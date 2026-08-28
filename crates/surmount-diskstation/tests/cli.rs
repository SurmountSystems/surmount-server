//! Hermetic DiskStation CLI contracts. Fake gio/secret-tool/avahi. No live NAS.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn mount() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-diskstation-afp-mount")
}
fn discover() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-diskstation-discover")
}
fn copy_uid() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-copy-mailplus-uid")
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "surmount-diskstation-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn chmod_x(p: &std::path::Path) {
    let mut perms = fs::metadata(p).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(p, perms).unwrap();
}

const SYNTH: &str = "SYNTHETIC-AFP-PASSWORD-not-real-9f3c";

#[test]
fn mount_help() {
    let out = Command::new(mount()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("afp://hunter@DS1513.local/MailPlus"));
    assert!(t.contains("DS3018xs"));
    assert!(t.contains("synology-afp"));
    assert!(t.contains("secret-tool lookup"));
    assert!(!has_ipv4(&t));
}

#[test]
fn mount_reuses_existing() {
    let dir = temp_dir("reuse");
    let gvfs = dir.join("gvfs");
    let already = gvfs.join("afp-volume:host=DS1513.local,user=hunter,volume=MailPlus");
    fs::create_dir_all(already.join("@local/1029/1029/Maildir")).unwrap();
    let bindir = dir.join("bin");
    fs::create_dir_all(&bindir).unwrap();
    let st_log = dir.join("secret-tool.log");
    fs::write(
        bindir.join("secret-tool"),
        "#!/bin/sh\necho unexpected >&2; exit 1\n",
    )
    .unwrap();
    chmod_x(&bindir.join("secret-tool"));
    fs::write(bindir.join("gio"), "#!/bin/sh\nexit 0\n").unwrap();
    chmod_x(&bindir.join("gio"));
    let path = format!("{}:{}", bindir.display(), std::env::var("PATH").unwrap());
    let out = Command::new(mount())
        .args(["mount", "--host", "DS1513"])
        .env("PATH", path)
        .env("SURMOUNT_SECRET_TOOL", bindir.join("secret-tool"))
        .env("SURMOUNT_AFP_GIO", bindir.join("gio"))
        .env("SURMOUNT_AFP_GVFS_ROOT", &gvfs)
        .env("SURMOUNT_AFP_HINT_FILE", dir.join("no-hint"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    assert!(err.contains("reuse") || err.contains("already") || err.contains("mounted"));
    assert!(!st_log.exists() || fs::read_to_string(&st_log).unwrap_or_default().is_empty());
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!all.contains(SYNTH));
}

#[test]
fn mount_fresh_stdin_not_argv() {
    let dir = temp_dir("fresh");
    let gvfs = dir.join("gvfs");
    fs::create_dir_all(&gvfs).unwrap();
    let bindir = dir.join("bin");
    fs::create_dir_all(&bindir).unwrap();
    let st_log = dir.join("st.log");
    let gio_argv = dir.join("gio.argv");
    let gio_log = dir.join("gio.log");
    fs::write(
        bindir.join("secret-tool"),
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>{}\nif echo \" $* \" | grep -q ' {SYNTH} '; then echo leak >&2; exit 3; fi\nif [ \"$1\" = lookup ]; then printf '%s\\n' '{SYNTH}'; exit 0; fi\nif [ \"$1\" = store ]; then cat >/dev/null; exit 0; fi\nexit 2\n",
            st_log.display()
        ),
    )
    .unwrap();
    chmod_x(&bindir.join("secret-tool"));
    fs::write(
        bindir.join("gio"),
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>{argv}\nstdin=\"$(cat || true)\"\nprintf 'argv=%s stdin=%s\\n' \"$*\" \"$stdin\" >>{log}\nif [ \"$1\" = mount ] && [ \"$2\" != -l ] && [ \"$2\" != -u ]; then\n  host=DS1513.local\n  mkdir -p \"{gvfs}/afp-volume:host=$host,user=hunter,volume=MailPlus/@local\"\n  exit 0\nfi\nexit 0\n",
            argv = gio_argv.display(),
            log = gio_log.display(),
            gvfs = gvfs.display()
        ),
    )
    .unwrap();
    chmod_x(&bindir.join("gio"));
    for dummy in ["xclip", "pbcopy", "wl-copy"] {
        fs::write(
            bindir.join(dummy),
            "#!/bin/sh\ncat >/dev/null || true\nexit 0\n",
        )
        .unwrap();
        chmod_x(&bindir.join(dummy));
    }
    let path = format!("{}:{}", bindir.display(), std::env::var("PATH").unwrap());
    let out = Command::new(mount())
        .args(["mount", "--host", "DS1513"])
        .env("PATH", path)
        .env("SURMOUNT_SECRET_TOOL", bindir.join("secret-tool"))
        .env("SURMOUNT_AFP_GIO", bindir.join("gio"))
        .env("SURMOUNT_AFP_GVFS_ROOT", &gvfs)
        .env("SURMOUNT_AFP_HINT_FILE", dir.join("no-hint"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let st = fs::read_to_string(&st_log).unwrap_or_default();
    assert!(st.contains("lookup") && st.contains("synology-afp") && st.contains("DS1513"));
    let argv = fs::read_to_string(&gio_argv).unwrap_or_default();
    assert!(argv.contains("afp://hunter@DS1513.local/MailPlus"));
    assert!(!argv.contains(SYNTH));
    let glog = fs::read_to_string(&gio_log).unwrap_or_default();
    assert!(glog.contains(&format!("stdin={SYNTH}")));
}

#[test]
fn discover_help_and_fake_avahi() {
    let out = Command::new(discover()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("DS1513") && t.contains("DS3018xs"));
    assert!(
        t.to_ascii_lowercase().contains("avahi") || t.contains("mDNS") || t.contains("Bonjour")
    );
    assert!(!has_ipv4(&t));

    let dir = temp_dir("avahi");
    let bindir = dir.join("bin");
    fs::create_dir_all(&bindir).unwrap();
    let avahi_out = dir.join("avahi-browse.out");
    fs::write(
        &avahi_out,
        r#"+;eth0;IPv4;DiskStation;_afpovertcp._tcp;local
=;eth0;IPv4;DiskStation;_afpovertcp._tcp;local;DiskStation.local;192.0.2.10;548;"model=DS1513+"
+;eth0;IPv4;DiskStation;_afpovertcp._tcp;local
=;eth0;IPv4;DiskStation;_afpovertcp._tcp;local;DiskStation-1.local;192.0.2.20;548;"model=DS3018xs"
"#,
    )
    .unwrap();
    fs::write(
        bindir.join("avahi-browse"),
        format!("#!/bin/sh\ncat {}\n", avahi_out.display()),
    )
    .unwrap();
    chmod_x(&bindir.join("avahi-browse"));
    fs::write(
        bindir.join("getent"),
        "#!/bin/sh\ncase \"$2\" in DS1513.local) echo '192.0.2.10 DS1513.local';; DS3018xs.local) echo '192.0.2.20 DS3018xs.local';; *) exit 2;; esac\n",
    )
    .unwrap();
    chmod_x(&bindir.join("getent"));
    let path = format!("{}:{}", bindir.display(), std::env::var("PATH").unwrap());
    let out = Command::new(discover())
        .env("PATH", &path)
        .env("SURMOUNT_AVAHI_BROWSE", bindir.join("avahi-browse"))
        .env("SURMOUNT_GETENT", bindir.join("getent"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.contains("host_id=DS1513"));
    assert!(t.contains("host_id=DS3018xs"));
    assert!(!t.contains("192.0.2."));
    assert!(t.to_ascii_lowercase().contains("mailplus"));
}

#[test]
fn copy_uid_dry_run() {
    let dir = temp_dir("copy");
    let gvfs = dir.join("gvfs");
    let mp = gvfs.join("afp-volume:host=diskstation.local,user=hunter,volume=MailPlus");
    fs::create_dir_all(mp.join("@local/1042/1042/Maildir/cur")).unwrap();
    fs::create_dir_all(mp.join("@local/1042/1042/Maildir/new")).unwrap();
    fs::create_dir_all(mp.join("@local/1042/1042/Maildir/.Sent/cur")).unwrap();
    fs::create_dir_all(mp.join("@local/1042/1042/Maildir/.All Mail/cur")).unwrap();
    fs::write(
        mp.join("@local/1042/1042/Maildir/cur/.gio-list"),
        "1710000000.M123P1.diskstation/2,S\n",
    )
    .unwrap();
    fs::write(
        mp.join("@local/1042/1042/Maildir/.Sent/cur/.gio-list"),
        "1710000001.M124P2.diskstation/2,\n",
    )
    .unwrap();
    fs::write(
        mp.join("@local/1042/1042/Maildir/.All Mail/cur/.gio-list"),
        "SHOULD-EXCLUDE\n",
    )
    .unwrap();
    let bindir = dir.join("bin");
    fs::create_dir_all(&bindir).unwrap();
    fs::write(
        bindir.join("gio"),
        r#"#!/bin/sh
if [ "$1" = list ]; then
  t="$2"
  if [ -f "$t/.gio-list" ]; then cat "$t/.gio-list"; fi
  exit 0
fi
exit 2
"#,
    )
    .unwrap();
    chmod_x(&bindir.join("gio"));
    let envf = dir.join("agent-target.env");
    fs::write(&envf, "export SURMOUNT_DEPLOY_TARGET=root@example.test\n").unwrap();
    let path = format!("{}:{}", bindir.display(), std::env::var("PATH").unwrap());
    let help = Command::new(copy_uid()).arg("--help").output().unwrap();
    let ht = String::from_utf8_lossy(&help.stdout);
    assert!(
        ht.contains("MailPlus/@local")
            && ht.contains("import/maildir")
            && ht.to_ascii_lowercase().contains("gio list")
    );
    assert!(!has_ipv4(&ht));

    let missing = Command::new(copy_uid())
        .arg("--dry-run")
        .env("SURMOUNT_AFP_GIO", bindir.join("gio"))
        .env("SURMOUNT_AFP_GVFS_ROOT", &gvfs)
        .env("SURMOUNT_AGENT_TARGET_ENV", &envf)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(!missing.status.success());

    let bad = Command::new(copy_uid())
        .args(["--dry-run", "jeff"])
        .env("SURMOUNT_AFP_GIO", bindir.join("gio"))
        .env("SURMOUNT_AFP_GVFS_ROOT", &gvfs)
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(!bad.status.success());
    let err = String::from_utf8_lossy(&bad.stderr).to_ascii_lowercase();
    assert!(err.contains("uid") || err.contains("numeric"));

    let out = Command::new(copy_uid())
        .args(["--dry-run", "1042"])
        .env("PATH", &path)
        .env("SURMOUNT_AFP_GIO", bindir.join("gio"))
        .env("SURMOUNT_AFP_GVFS_ROOT", &gvfs)
        .env("SURMOUNT_AGENT_TARGET_ENV", &envf)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.contains("/var/lib/surmount/import/maildir/1042/1042/Maildir"));
    assert!(t.contains("cur/1710000000.M123P1.diskstation:2,S"));
    if t.contains("All Mail") {
        assert!(t.to_ascii_lowercase().contains("exclude"));
    }
    assert!(!has_ipv4(&t));
}

fn has_ipv4(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut dots = 0;
            let mut j = i;
            let mut oct = 0;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                if bytes[j] == b'.' {
                    dots += 1;
                    oct = 0;
                } else {
                    oct += 1;
                    if oct > 3 {
                        break;
                    }
                }
                j += 1;
            }
            if dots == 3 && oct > 0 {
                return true;
            }
        }
        i += 1;
    }
    false
}
