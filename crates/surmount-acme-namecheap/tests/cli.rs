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

    let wrap_ssh = bin.join("ssh-operator");
    fs::write(&wrap_ssh, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&wrap_ssh, fs::Permissions::from_mode(0o755)).unwrap();
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
            "--ssh-cmd",
            wrap_ssh.to_str().unwrap(),
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    let log = fs::read_to_string(&install_log).unwrap();
    assert!(
        log.contains("--ssh-cmd") && log.contains(wrap_ssh.to_str().unwrap()),
        "install must forward --ssh-cmd so IdentitiesOnly nixbuilder is not the PEM target: {log}"
    );
}

/// Extra-zone DNS-01 wrap must exec the stored `--hook` (dispatcher), not the
/// one-SLD Namecheap hook alone. Extra zones (cryptoquick.com and others) cannot
/// write TXT through the surmount.systems env.
#[test]
fn laptop_issue_wrap_execs_stored_hook_not_one_sld_only() {
    let root = scratch("laptop-hook");
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let envp = root.join("namecheap.env");
    fs::write(
        &envp,
        "ApiUser=hermetic-user\nApiKey=SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-bb22\nUserName=hermetic-user\nClientIp=192.0.2.1\nSettleSeconds=120\n",
    )
    .unwrap();
    fs::set_permissions(&envp, fs::Permissions::from_mode(0o600)).unwrap();
    let fake_ui = bin.join("ui");
    let write_fake_ui = |issue_dir: &PathBuf| {
        fs::create_dir_all(issue_dir).unwrap();
        fs::write(
            &fake_ui,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' '{}BEGIN CERTIFICATE{}' >\"$SURMOUNT_TLS_CERT\"\nprintf '%s\\n' '{}BEGIN PRIVATE KEY{}' >\"$SURMOUNT_TLS_KEY\"\nchmod 0600 \"$SURMOUNT_TLS_CERT\" \"$SURMOUNT_TLS_KEY\"\nsleep 30\n",
                "-".repeat(5),
                "-".repeat(5),
                "-".repeat(5),
                "-".repeat(5),
            ),
        )
        .unwrap();
        fs::set_permissions(&fake_ui, fs::Permissions::from_mode(0o755)).unwrap();
    };
    let one_sld_exec = "exec acme-dns-hook-namecheap \"$@\"";
    let extra_domains = "surmount.systems,www.surmount.systems,mail.surmount.systems,services.surmount.systems,mta-sts.surmount.systems,baxterartworks.com,www.baxterartworks.com,btcfur.com,www.btcfur.com,exophiles.org,www.exophiles.org,iantuckerstudios.com,www.iantuckerstudios.com,nostrfurs.com,www.nostrfurs.com,yiffa.app,www.yiffa.app,cryptoquick.com,www.cryptoquick.com,mail.cryptoquick.com";

    let stored = PathBuf::from(dispatch());
    let issue_hook = root.join("issue-hook");
    write_fake_ui(&issue_hook);
    fs::create_dir_all(issue_hook.join("tls")).unwrap();
    fs::write(issue_hook.join("tls/cert.pem"), "OLD-LEAF-MUST-NOT-KEEP\n").unwrap();
    fs::write(issue_hook.join("tls/key.pem"), "OLD-KEY-MUST-NOT-KEEP\n").unwrap();
    let out = Command::new(laptop())
        .args([
            "--issue",
            "--directory",
            "production",
            "--namecheap-env",
            envp.to_str().unwrap(),
            "--domains",
            extra_domains,
            "--work-dir",
            issue_hook.to_str().unwrap(),
            "--ui-bin",
            fake_ui.to_str().unwrap(),
            "--hook",
            stored.to_str().unwrap(),
            "--timeout-secs",
            "10",
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    let new_cert = fs::read_to_string(issue_hook.join("tls/cert.pem")).unwrap();
    assert!(
        new_cert.contains("BEGIN CERTIFICATE"),
        "--issue must replace leftover PEMs so missing certificate hostnames can land"
    );
    assert!(
        !new_cert.contains("OLD-LEAF-MUST-NOT-KEEP"),
        "--issue must not treat leftover PEMs as success"
    );
    let wrap_s = fs::read_to_string(issue_hook.join("acme-dns-hook-wrap.sh")).unwrap();
    assert!(
        wrap_s.contains("export HOME="),
        "wrap must re-export HOME after ACME env_clear so dispatch finds per-zone env"
    );
    assert!(
        wrap_s.lines().any(|l| {
            let t = l.trim();
            t.starts_with("exec ") && t.contains(stored.to_str().unwrap()) && t.contains("\"$@\"")
        }),
        "wrap must exec the stored --hook (dispatcher): {wrap_s}"
    );
    assert!(
        !wrap_s.lines().any(|l| l.trim() == one_sld_exec),
        "wrap must not exec only the one-SLD hook when --hook is the dispatcher: {wrap_s}"
    );
    let inner = PathBuf::from(hook());
    assert!(
        wrap_s.contains("SURMOUNT_ACME_DNS_DISPATCH_INNER_HOOK")
            && wrap_s.contains(inner.to_str().unwrap()),
        "dispatcher wrap must point INNER_HOOK at the one-SLD bin (sibling), not PATH: {wrap_s}"
    );

    let issue_extra = root.join("issue-extra");
    write_fake_ui(&issue_extra);
    let out = Command::new(laptop())
        .args([
            "--issue",
            "--directory",
            "production",
            "--namecheap-env",
            envp.to_str().unwrap(),
            "--domains",
            extra_domains,
            "--work-dir",
            issue_extra.to_str().unwrap(),
            "--ui-bin",
            fake_ui.to_str().unwrap(),
            "--timeout-secs",
            "10",
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    let wrap_s = fs::read_to_string(issue_extra.join("acme-dns-hook-wrap.sh")).unwrap();
    assert!(
        wrap_s.contains("acme-dns-hook-namecheap-dispatch"),
        "extra-zone --issue without --hook must default wrap exec to the dispatcher: {wrap_s}"
    );
    assert!(
        !wrap_s.lines().any(|l| l.trim() == one_sld_exec),
        "extra-zone wrap must not exec only the one-SLD hook: {wrap_s}"
    );
    assert!(
        wrap_s.contains("hook-stderr.log"),
        "wrap must keep hook stderr so Invalid request IP can surface: {wrap_s}"
    );
}

/// --issue must fail when the UI/hook child exits non-zero before PEMs exist.
#[test]
fn laptop_issue_fails_when_ui_child_exits_nonzero() {
    let root = scratch("laptop-ui-fail");
    let envp = root.join("namecheap.env");
    fs::write(
        &envp,
        "ApiUser=hermetic-user\nApiKey=SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-cc33\nUserName=hermetic-user\nClientIp=192.0.2.1\nSettleSeconds=120\n",
    )
    .unwrap();
    fs::set_permissions(&envp, fs::Permissions::from_mode(0o600)).unwrap();
    let fake_ui = root.join("ui-fail");
    fs::write(&fake_ui, "#!/bin/sh\nexit 7\n").unwrap();
    fs::set_permissions(&fake_ui, fs::Permissions::from_mode(0o755)).unwrap();
    let work = root.join("work");
    fs::create_dir_all(&work).unwrap();
    let out = Command::new(laptop())
        .args([
            "--issue",
            "--directory",
            "production",
            "--namecheap-env",
            envp.to_str().unwrap(),
            "--domains",
            "services.surmount.systems",
            "--work-dir",
            work.to_str().unwrap(),
            "--ui-bin",
            fake_ui.to_str().unwrap(),
            "--timeout-secs",
            "8",
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0, "{}", combined(&out));
    let t = combined(&out).to_ascii_lowercase();
    assert!(
        t.contains("non-zero") || t.contains("exited"),
        "must fail --issue on ui-bin/hook child non-zero: {}",
        combined(&out)
    );
    assert!(!combined(&out).contains("SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-cc33"));
}

/// Host-profile Cloudflare extras must not replace Namecheap static zones.
/// Profile without --domains floors the intended 20-name leaf.
#[test]
fn host_profile_floors_intended_leaf_and_strips_cloudflare_extras() {
    let root = scratch("laptop-profile-floor");
    let envp = root.join("namecheap.env");
    fs::write(
        &envp,
        "ApiUser=hermetic-user\nApiKey=SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-dd44\nUserName=hermetic-user\nClientIp=192.0.2.1\nSettleSeconds=120\n",
    )
    .unwrap();
    fs::set_permissions(&envp, fs::Permissions::from_mode(0o600)).unwrap();
    let profile = root.join("host-profile.toml");
    fs::write(
        &profile,
        "host_id = \"surmount-1\"\nacme_email = \"admin@example.invalid\"\nacme_domains = [\n  \"surmount.systems\",\n  \"www.surmount.systems\",\n  \"mail.surmount.systems\",\n  \"services.surmount.systems\",\n  \"mta-sts.surmount.systems\",\n  \"cryptoquick.com\",\n  \"www.cryptoquick.com\",\n  \"mail.cryptoquick.com\",\n  \"btcdragonlord.com\",\n  \"www.btcdragonlord.com\",\n  \"btckitties.com\",\n  \"www.btckitties.com\",\n]\n",
    )
    .unwrap();
    let out = Command::new(laptop())
        .args([
            "--directory",
            "production",
            "--namecheap-env",
            envp.to_str().unwrap(),
            "--host-profile",
            profile.to_str().unwrap(),
            "--dry-run",
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    let t = combined(&out);
    assert!(
        t.contains("exophiles.org") && t.contains("www.exophiles.org"),
        "host-profile must floor exophiles even when the profile omitted them: {t}"
    );
    assert!(
        !t.contains("btcdragonlord.com") && !t.contains("btckitties.com"),
        "host-profile must strip Cloudflare extras: {t}"
    );
    assert!(!t.contains("SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-dd44"));
}

/// Explicit extra-zone --issue that omits exophiles must fail closed.
#[test]
fn issue_refuses_extra_zone_list_that_drops_exophiles() {
    let root = scratch("laptop-drop-exophiles");
    let envp = root.join("namecheap.env");
    fs::write(
        &envp,
        "ApiUser=hermetic-user\nApiKey=SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-ee55\nUserName=hermetic-user\nClientIp=192.0.2.1\nSettleSeconds=120\n",
    )
    .unwrap();
    fs::set_permissions(&envp, fs::Permissions::from_mode(0o600)).unwrap();
    let fake_ui = root.join("ui");
    fs::write(&fake_ui, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&fake_ui, fs::Permissions::from_mode(0o755)).unwrap();
    let work = root.join("work");
    fs::create_dir_all(&work).unwrap();
    let dropped = "surmount.systems,www.surmount.systems,mail.surmount.systems,services.surmount.systems,mta-sts.surmount.systems,baxterartworks.com,www.baxterartworks.com,btcfur.com,www.btcfur.com,iantuckerstudios.com,www.iantuckerstudios.com,nostrfurs.com,www.nostrfurs.com,yiffa.app,www.yiffa.app,cryptoquick.com,www.cryptoquick.com,mail.cryptoquick.com";
    let out = Command::new(laptop())
        .args([
            "--issue",
            "--directory",
            "production",
            "--namecheap-env",
            envp.to_str().unwrap(),
            "--domains",
            dropped,
            "--work-dir",
            work.to_str().unwrap(),
            "--ui-bin",
            fake_ui.to_str().unwrap(),
            "--timeout-secs",
            "4",
        ])
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .output()
        .unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0, "{}", combined(&out));
    let t = combined(&out).to_ascii_lowercase();
    assert!(
        t.contains("exophiles.org") && (t.contains("intended") || t.contains("drop")),
        "must refuse an 18-name extra-zone list that omitted exophiles: {}",
        combined(&out)
    );
    assert!(!combined(&out).contains("SURMOUNT-TEST-NAMECHEAP-APIKEY-do-not-log-ee55"));
}

struct FakeNc {
    records: Vec<(String, String, String, String, String)>,
    email_type: String,
    invalid_ip: bool,
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn url_decode(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < b.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    for part in q.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut kv = part.splitn(2, '=');
        let k = url_decode(kv.next().unwrap_or(""));
        let v = url_decode(kv.next().unwrap_or(""));
        m.insert(k, v);
    }
    m
}

fn spawn_namecheap_http(state: std::sync::Arc<std::sync::Mutex<FakeNc>>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 8192];
            let n = match std::io::Read::read(&mut stream, &mut buf) {
                Ok(n) => n,
                Err(_) => continue,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let first = req.lines().next().unwrap_or("");
            let path = first.split_whitespace().nth(1).unwrap_or("/");
            let q = path.split_once('?').map(|x| x.1).unwrap_or("");
            let params = parse_query(q);
            let body = {
                let mut g = state.lock().unwrap();
                if g.invalid_ip {
                    r#"<?xml version="1.0"?><ApiResponse Status="ERROR"><Errors><Error Number="1011150">Invalid request IP</Error></Errors></ApiResponse>"#.to_string()
                } else {
                    let cmd = params.get("Command").cloned().unwrap_or_default();
                    if cmd.contains("setHosts") {
                        let mut recs = Vec::new();
                        for i in 1..64 {
                            let name = params.get(&format!("HostName{i}")).cloned();
                            let Some(name) = name else { break };
                            recs.push((
                                name,
                                params
                                    .get(&format!("RecordType{i}"))
                                    .cloned()
                                    .unwrap_or_default(),
                                params
                                    .get(&format!("Address{i}"))
                                    .cloned()
                                    .unwrap_or_default(),
                                params
                                    .get(&format!("MXPref{i}"))
                                    .cloned()
                                    .unwrap_or_else(|| "10".into()),
                                params
                                    .get(&format!("TTL{i}"))
                                    .cloned()
                                    .unwrap_or_else(|| "1800".into()),
                            ));
                        }
                        if let Some(et) = params.get("EmailType") {
                            g.email_type = et.clone();
                        }
                        g.records = recs;
                        r#"<?xml version="1.0"?><ApiResponse Status="OK"><DomainDNSSetHostsResult IsSuccess="true" /></ApiResponse>"#.to_string()
                    } else {
                        let mut xml = String::from(
                            r#"<?xml version="1.0"?><ApiResponse Status="OK"><CommandResponse><DomainDNSGetHostsResult EmailType=""#,
                        );
                        xml.push_str(&xml_escape(&g.email_type));
                        xml.push_str(r#"">"#);
                        for (n, t, a, mx, ttl) in &g.records {
                            xml.push_str(&format!(
                                r#"<host Name="{}" Type="{}" Address="{}" MXPref="{}" TTL="{}" />"#,
                                xml_escape(n),
                                xml_escape(t),
                                xml_escape(a),
                                xml_escape(mx),
                                xml_escape(ttl)
                            ));
                        }
                        xml.push_str("</DomainDNSGetHostsResult></CommandResponse></ApiResponse>");
                        xml
                    }
                }
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = std::io::Write::write_all(&mut stream, resp.as_bytes());
        }
    });
    format!("http://{addr}/xml.response")
}

/// Live hook path (no MOCK_DIR) must getHosts then setHosts. Local HTTP only.
#[test]
fn hook_live_gethosts_sethosts_publishes_txt() {
    let root = scratch("hook-live");
    let cred = root.join("namecheap.env");
    write_cred(
        &cred,
        "example",
        "test",
        "surmount-test-apikey-not-real-live-001",
    );
    let state = std::sync::Arc::new(std::sync::Mutex::new(FakeNc {
        records: vec![(
            "@".into(),
            "A".into(),
            "203.0.113.10".into(),
            "10".into(),
            "1800".into(),
        )],
        email_type: "MX".into(),
        invalid_ip: false,
    }));
    let base = spawn_namecheap_http(state.clone());
    let cmd = || {
        let mut c = Command::new(hook());
        c.env("SURMOUNT_ACME_DNS_NAMECHEAP_ENV", &cred);
        c.env("SURMOUNT_ACME_DNS_NAMECHEAP_API_BASE", &base);
        c.env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR");
        c
    };
    let fqdn = "_acme-challenge.www.example.test";
    let txt = "synthetic-live-acme-txt-ddd444";
    let out = cmd().args(["set", fqdn, txt]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", combined(&out));
    assert!(!combined(&out).contains("surmount-test-apikey-not-real-live-001"));
    {
        let g = state.lock().unwrap();
        assert!(
            g.records
                .iter()
                .any(|(n, t, a, _, _)| n == "_acme-challenge.www" && t == "TXT" && a == txt),
            "setHosts must publish extra-name TXT: {:?}",
            g.records
        );
        assert!(
            g.records.iter().any(|(n, t, _, _, _)| n == "@" && t == "A"),
            "setHosts must re-apply other getHosts records"
        );
        assert_eq!(g.email_type, "MX");
    }
    assert_eq!(
        cmd().args(["wait", fqdn, txt]).status().unwrap().code(),
        Some(0)
    );
    assert_eq!(
        cmd().args(["clear", fqdn]).status().unwrap().code(),
        Some(0)
    );
    let g = state.lock().unwrap();
    assert!(
        !g.records
            .iter()
            .any(|(n, t, _, _, _)| n == "_acme-challenge.www" && t == "TXT")
    );
    assert!(g.records.iter().any(|(n, t, _, _, _)| n == "@" && t == "A"));
}

#[test]
fn hook_live_invalid_request_ip_is_blocked() {
    let root = scratch("hook-ip");
    let cred = root.join("namecheap.env");
    write_cred(
        &cred,
        "example",
        "test",
        "surmount-test-apikey-not-real-live-002",
    );
    let state = std::sync::Arc::new(std::sync::Mutex::new(FakeNc {
        records: vec![(
            "@".into(),
            "A".into(),
            "203.0.113.10".into(),
            "10".into(),
            "1800".into(),
        )],
        email_type: "MX".into(),
        invalid_ip: true,
    }));
    let base = spawn_namecheap_http(state);
    let out = Command::new(hook())
        .args(["set", "_acme-challenge.example.test", "synthetic-txt"])
        .env("SURMOUNT_ACME_DNS_NAMECHEAP_ENV", &cred)
        .env("SURMOUNT_ACME_DNS_NAMECHEAP_API_BASE", &base)
        .env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR")
        .output()
        .unwrap();
    assert_ne!(out.status.code().unwrap_or(0), 0);
    let t = combined(&out);
    assert!(t.to_ascii_lowercase().contains("invalid request ip"));
    assert!(t.contains("BLOCKED"));
    assert!(!t.contains("surmount-test-apikey-not-real-live-002"));
}
