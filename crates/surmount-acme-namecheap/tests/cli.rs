//! Hermetic tests for hook, dispatch, lab, render, laptop-renew.
//! No network, no live Namecheap, no live Let's Encrypt.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn hook() -> &'static str {
    env!("CARGO_BIN_EXE_acme-dns-hook-namecheap")
}
fn dispatch() -> &'static str {
    env!("CARGO_BIN_EXE_acme-dns-hook-namecheap-dispatch")
}
fn lab() -> &'static str {
    env!("CARGO_BIN_EXE_acme-dns-hook-lab")
}
fn render() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-render-host-profile-acme")
}
fn laptop() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-laptop-renew-cert")
}

fn scratch(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-acme-{}-{}-{}",
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

fn write_cred(path: &PathBuf, sld: &str, tld: &str, key: &str) {
    fs::write(
        path,
        format!(
            "ApiUser=surmount-test-apiuser\nApiKey={key}\nUserName=surmount-test-apiuser\nClientIp=203.0.113.50\nSLD={sld}\nTLD={tld}\nSettleSeconds=0\nTxtTtl=60\n"
        ),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn hook_set_wait_clear_preserve() {
    let root = scratch("hook");
    let mock = root.join("mock");
    fs::create_dir_all(&mock).unwrap();
    let cred = root.join("namecheap.env");
    write_cred(
        &cred,
        "example",
        "test",
        "surmount-test-apikey-not-real-001",
    );
    fs::write(
        mock.join("hosts.txt"),
        "@|A|203.0.113.10|10|1800\nwww|CNAME|example.test.|10|1800\nmail|A|203.0.113.11|10|1800\n",
    )
    .unwrap();
    let cmd = || {
        let mut c = Command::new(hook());
        c.env("SURMOUNT_ACME_DNS_NAMECHEAP_ENV", &cred);
        c.env("SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR", &mock);
        c.env("SURMOUNT_ACME_DNS_NAMECHEAP_VERBOSE", "0");
        c
    };
    let out = cmd().output().unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    let fqdn = "_acme-challenge.services.example.test";
    let txt1 = "synthetic-acme-txt-value-aaa111";
    let txt2 = "synthetic-acme-txt-value-bbb222";
    assert_eq!(
        cmd().args(["set", fqdn, txt1]).status().unwrap().code(),
        Some(0)
    );
    assert_eq!(
        cmd().args(["wait", fqdn, txt1]).status().unwrap().code(),
        Some(0)
    );
    let hosts = fs::read_to_string(mock.join("hosts.txt")).unwrap();
    assert!(hosts.contains(&format!("_acme-challenge.services|TXT|{txt1}|")));
    assert!(hosts.contains("@|A|203.0.113.10|"));
    assert_eq!(
        cmd().args(["set", fqdn, txt2]).status().unwrap().code(),
        Some(0)
    );
    assert_eq!(
        fs::read_to_string(mock.join("hosts.txt"))
            .unwrap()
            .matches("_acme-challenge.services|TXT|")
            .count(),
        1
    );
    assert_eq!(
        cmd().args(["wait", fqdn, txt1]).status().unwrap().code(),
        Some(1)
    );
    assert_eq!(
        cmd().args(["clear", fqdn]).status().unwrap().code(),
        Some(0)
    );
    let hosts = fs::read_to_string(mock.join("hosts.txt")).unwrap();
    assert!(!hosts.contains("_acme-challenge.services|TXT|"));
    assert!(hosts.contains("mail|A|203.0.113.11|"));
    assert_eq!(
        cmd()
            .args([
                "set",
                "_acme-challenge.example.test",
                "synthetic-apex-txt-ccc333"
            ])
            .status()
            .unwrap()
            .code(),
        Some(0)
    );
    assert!(
        fs::read_to_string(mock.join("hosts.txt"))
            .unwrap()
            .contains("_acme-challenge|TXT|synthetic-apex-txt-ccc333|")
    );
    let out = cmd()
        .args(["set", "_acme-challenge.other.invalid", txt1])
        .output()
        .unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    assert!(combined(&out).to_ascii_lowercase().contains("not under"));
    let out = cmd().args(["bounce", fqdn]).output().unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    fs::set_permissions(&cred, fs::Permissions::from_mode(0o644)).unwrap();
    let out = cmd().args(["set", fqdn, txt1]).output().unwrap();
    fs::set_permissions(&cred, fs::Permissions::from_mode(0o600)).unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    assert!(
        combined(&out).to_ascii_lowercase().contains("owner-only")
            || combined(&out).contains("0600")
    );
    assert!(!combined(&out).contains("surmount-test-apikey-not-real-001"));
}

#[test]
fn dispatch_maps_zones() {
    let root = scratch("disp");
    let home = root.join("home");
    let xdg = root.join("xdg");
    let mock = root.join("mock");
    fs::create_dir_all(xdg.join("surmount/issue-le-prod")).unwrap();
    fs::create_dir_all(xdg.join("surmount/namecheap")).unwrap();
    fs::create_dir_all(home.join(".local/share/surmount/namecheap")).unwrap();
    fs::create_dir_all(&mock).unwrap();
    let primary = xdg.join("surmount/issue-le-prod/namecheap.env");
    let extra = xdg.join("surmount/namecheap/cryptoquick.com.env");
    write_cred(
        &primary,
        "surmount",
        "systems",
        "surmount-test-apikey-not-real-dispatch-001",
    );
    write_cred(
        &extra,
        "cryptoquick",
        "com",
        "surmount-test-apikey-not-real-dispatch-001",
    );
    fs::write(mock.join("hosts.txt"), "@|A|203.0.113.10|10|1800\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(dispatch())
            .args(args)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &xdg)
            .env("SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR", &mock)
            .env("SURMOUNT_ACME_DNS_DISPATCH_INNER_HOOK", hook())
            .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
            .output()
            .unwrap()
    };
    assert_ne!(run(&[]).status.code().unwrap_or(0), 0);
    let out = run(&[
        "set",
        "_acme-challenge.services.surmount.systems",
        "synthetic-dispatch-txt-ss-aaa",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        fs::read_to_string(mock.join("hosts.txt"))
            .unwrap()
            .contains("_acme-challenge.services|TXT|synthetic-dispatch-txt-ss-aaa|")
    );
    assert_eq!(
        run(&[
            "wait",
            "_acme-challenge.services.surmount.systems",
            "synthetic-dispatch-txt-ss-aaa"
        ])
        .status
        .code(),
        Some(0)
    );
    assert_eq!(
        run(&["clear", "_acme-challenge.services.surmount.systems"])
            .status
            .code(),
        Some(0)
    );
    let out = run(&[
        "set",
        "_acme-challenge.cryptoquick.com",
        "synthetic-dispatch-txt-cq-bbb",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    assert!(
        fs::read_to_string(mock.join("hosts.txt"))
            .unwrap()
            .contains("_acme-challenge|TXT|synthetic-dispatch-txt-cq-bbb|")
    );
    let before = fs::read_to_string(mock.join("hosts.txt")).unwrap();
    let out = run(&[
        "set",
        "_acme-challenge.yiffa.app",
        "synthetic-dispatch-txt-yi-ccc",
    ]);
    assert_ne!(out.status.code().unwrap_or(0), 0);
    assert_eq!(fs::read_to_string(mock.join("hosts.txt")).unwrap(), before);
    assert!(!combined(&out).contains("surmount-test-apikey-not-real-dispatch-001"));
    let out = run(&[
        "set",
        "_acme-challenge.www.cryptoquick.com",
        "synthetic-dispatch-txt-www-ddd",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    assert!(
        fs::read_to_string(mock.join("hosts.txt"))
            .unwrap()
            .contains("_acme-challenge.www|TXT|synthetic-dispatch-txt-www-ddd|")
    );
    let link = xdg.join("surmount/namecheap/yiffa.app.env");
    std::os::unix::fs::symlink(&extra, &link).unwrap();
    let out = run(&[
        "set",
        "_acme-challenge.yiffa.app",
        "synthetic-dispatch-txt-yi-ccc",
    ]);
    assert_ne!(out.status.code().unwrap_or(0), 0);
}

#[test]
fn lab_set_wait_clear() {
    let root = scratch("lab");
    let zone = root.join("zone.txt");
    let run = |args: &[&str]| {
        Command::new(lab())
            .args(args)
            .env("SURMOUNT_ACME_DNS_LAB_ZONE", &zone)
            .output()
            .unwrap()
    };
    assert_eq!(
        run(&["set", "_acme-challenge.example.test", "lab-txt-1"])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        run(&["wait", "_acme-challenge.example.test", "lab-txt-1"])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        run(&["clear", "_acme-challenge.example.test"])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        run(&["wait", "_acme-challenge.example.test", "lab-txt-1"])
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn render_sample_and_refuse() {
    let root = scratch("render");
    let profile = root.join("sample.toml");
    fs::write(
        &profile,
        include_str!("../testdata/host-profile/sample-host-profile.toml"),
    )
    .unwrap();
    let out = Command::new(render())
        .args(["--profile", profile.to_str().unwrap(), "--stdout"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = combined(&out);
    assert!(text.contains("enable = true"));
    assert!(text.contains("services.example.invalid"));
    assert!(text.contains("mail.example.invalid"));
    assert!(text.contains("www.example.invalid"));
    assert!(text.contains("ops@example.invalid"));
    assert!(text.contains("acme-staging-v02.api.letsencrypt.org"));
    assert!(text.contains("dnsProvider = \"external-hook\""));
    assert!(text.contains("dnsHookPath = \"/run/surmount/acme-dns-hook\""));
    let out_dir = root.join("private-host-local");
    let st = Command::new(render())
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(st.success());
    assert!(out_dir.join("host-local-acme.nix").is_file());
    let out = Command::new(render())
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--out",
            out_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    assert!(
        combined(&out).to_ascii_lowercase().contains("exists") || combined(&out).contains("force")
    );
    let hosts = root.join("hosts");
    fs::create_dir_all(&hosts).unwrap();
    let out = Command::new(render())
        .args([
            "--profile",
            profile.to_str().unwrap(),
            "--out",
            hosts.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    let bad = root.join("no-email.toml");
    fs::write(&bad, "acme_domains = [ \"services.example.invalid\" ]\n").unwrap();
    let out = Command::new(render())
        .args(["--profile", bad.to_str().unwrap(), "--stdout"])
        .output()
        .unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    assert!(combined(&out).contains("acme_email"));
}

#[test]
fn laptop_blocked_print_issue_install_check() {
    let root = scratch("laptop");
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let envp = root.join("namecheap.env");
    let plant = "SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-aa11";
    fs::write(
        &envp,
        format!("ApiUser=hermetic-user\nApiKey={plant}\nUserName=hermetic-user\nClientIp=192.0.2.1\nSettleSeconds=120\n"),
    )
    .unwrap();
    fs::set_permissions(&envp, fs::Permissions::from_mode(0o600)).unwrap();
    let run = |args: &[&str]| {
        Command::new(laptop())
            .args(args)
            .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
            .output()
            .unwrap()
    };
    let out = run(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    let t = combined(&out);
    assert!(t.contains("--directory"));
    assert!(t.contains("--namecheap-env"));
    assert!(t.to_ascii_lowercase().contains("host acme stays off"));
    assert!(t.contains("--check") && t.contains("--live") && t.contains("--install-timer"));
    let out = run(&["--namecheap-env", envp.to_str().unwrap(), "--dry-run"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(combined(&out).to_ascii_lowercase().contains("never silent"));
    let missing = root.join("missing.env");
    let out = run(&[
        "--directory",
        "production",
        "--namecheap-env",
        missing.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(combined(&out).to_ascii_lowercase().contains("laptop"));
    fs::set_permissions(&envp, fs::Permissions::from_mode(0o640)).unwrap();
    let out = run(&[
        "--directory",
        "production",
        "--namecheap-env",
        envp.to_str().unwrap(),
        "--dry-run",
    ]);
    fs::set_permissions(&envp, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = run(&[
        "--directory",
        "production",
        "--namecheap-env",
        envp.to_str().unwrap(),
        "--domains",
        "services.example.invalid,example.invalid,www.example.invalid,mta-sts.example.invalid",
        "--dry-run",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    let t = combined(&out);
    assert!(t.contains("acme-v02.api.letsencrypt.org/directory"));
    assert!(t.contains("directory=production"));
    assert!(!t.contains("acme-staging-v02"));
    assert!(t.contains("mta-sts.example.invalid"));
    assert!(t.to_ascii_lowercase().contains("host acme stays off"));
    assert!(!t.contains(plant));
    assert!(!t.contains("192.0.2.1"));
    let out = run(&[
        "--directory",
        "staging",
        "--namecheap-env",
        envp.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(combined(&out).contains("acme-staging-v02.api.letsencrypt.org/directory"));
    let low = root.join("low.env");
    fs::write(
        &low,
        format!("ApiUser=hermetic-user\nApiKey={plant}\nSettleSeconds=30\n"),
    )
    .unwrap();
    fs::set_permissions(&low, fs::Permissions::from_mode(0o600)).unwrap();
    let out = run(&[
        "--directory",
        "production",
        "--namecheap-env",
        low.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let link = root.join("namecheap.link");
    std::os::unix::fs::symlink(&envp, &link).unwrap();
    let out = run(&[
        "--directory",
        "production",
        "--namecheap-env",
        link.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(out.status.code(), Some(1));

    let fake_ui = bin.join("surmount-management-ui");
    let issue_dir = root.join("issue");
    fs::create_dir_all(&issue_dir).unwrap();
    fs::write(
        &fake_ui,
        format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' '{}BEGIN CERTIFICATE{}' >\"$SURMOUNT_TLS_CERT\"\nprintf '%s\\n' '{}BEGIN PRIVATE KEY{}' >\"$SURMOUNT_TLS_KEY\"\nchmod 0600 \"$SURMOUNT_TLS_CERT\" \"$SURMOUNT_TLS_KEY\"\nprintf '%s\\n' \"enable=${{SURMOUNT_ACME_ENABLE-}} dir=${{SURMOUNT_ACME_DIRECTORY-}} hook=${{SURMOUNT_ACME_DNS_HOOK-}}\" >{}/ui-env.txt\nsleep 30\n",
            "-".repeat(5),
            "-".repeat(5),
            "-".repeat(5),
            "-".repeat(5),
            issue_dir.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake_ui, fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(laptop())
        .args([
            "--issue",
            "--directory",
            "production",
            "--namecheap-env",
            envp.to_str().unwrap(),
            "--domains",
            "services.example.invalid,mta-sts.example.invalid",
            "--work-dir",
            issue_dir.to_str().unwrap(),
            "--ui-bin",
            fake_ui.to_str().unwrap(),
            "--timeout-secs",
            "10",
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    assert!(issue_dir.join("tls/cert.pem").is_file());
    assert!(!combined(&out).contains(plant));
    let ui_env = fs::read_to_string(issue_dir.join("ui-env.txt")).unwrap();
    assert!(ui_env.contains("acme-v02.api.letsencrypt.org/directory"));
    let wrap = issue_dir.join("acme-dns-hook-wrap.sh");
    let wrap_s = fs::read_to_string(&wrap).unwrap();
    assert!(wrap_s.contains("SURMOUNT_ACME_DNS_NAMECHEAP_ENV"));
    assert!(wrap_s.contains(envp.to_str().unwrap()));
    assert!(wrap_s.contains("export HOME="));
    assert!(ui_env.contains(&format!("hook={}", wrap.display())));

    let fake_install = bin.join("secrets-install-host.sh");
    let install_log = root.join("install.log");
    fs::write(
        &fake_install,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >{}\n",
            install_log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake_install, fs::Permissions::from_mode(0o755)).unwrap();
    let out = Command::new(laptop())
        .args([
            "--install",
            "--directory",
            "production",
            "--namecheap-env",
            envp.to_str().unwrap(),
            "--work-dir",
            issue_dir.to_str().unwrap(),
            "--secrets-install",
            fake_install.to_str().unwrap(),
            "--host-id",
            "mail-lab",
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    let log = fs::read_to_string(&install_log).unwrap();
    assert!(log.contains("--require-kind tls-cert"));
    assert!(log.contains("--require-kind tls-key"));
}
