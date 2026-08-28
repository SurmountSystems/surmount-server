//! Hermetic batch-mode CLI tests. No live keyring. Synthetic secrets only.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-secrets-prompt")
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-secrets-prompt-batch-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

/// Prepend dummy wipe binaries so store tests never touch the live clipboard.
fn path_with_wipe_stubs(dir: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let bin_dir = dir.join("wipe-stubs");
    fs::create_dir_all(&bin_dir).unwrap();
    let stub = "#!/bin/sh\nset -eu\ncat >/dev/null || true\nexit 0\n";
    for name in ["xclip", "pbcopy", "wl-copy"] {
        let p = bin_dir.join(name);
        fs::write(&p, stub).unwrap();
        let mut perm = fs::metadata(&p).unwrap().permissions();
        perm.set_mode(0o700);
        fs::set_permissions(&p, perm).unwrap();
    }
    format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn batch_namecheap_staging_shape() {
    let staging = temp_dir("nc");
    let secret_file = staging.join("api-key.txt");
    fs::write(&secret_file, "SYNTHETIC-BATCH-KEY-9f3c2a1b").unwrap();

    let out = Command::new(bin())
        .args([
            "namecheap-api",
            "--host",
            "mail-lab",
            "--batch",
            "--staging",
            staging.to_str().unwrap(),
            "--api-user",
            "labuser",
            "--client-ip",
            "203.0.113.10",
            "--sld",
            "example",
            "--tld",
            "test",
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let item = staging.join("namecheap-api");
    let attrs = fs::read_to_string(item.join("attributes")).unwrap();
    let secret = fs::read_to_string(item.join("secret")).unwrap();

    assert!(attrs.contains("surmount.kind=namecheap-api\n"));
    assert!(attrs.contains("surmount.host=mail-lab\n"));
    assert!(attrs.contains("surmount.path=/var/lib/surmount/secrets/acme/namecheap.env\n"));
    assert!(secret.contains("ApiUser=labuser\n"));
    assert!(secret.contains("ApiKey=SYNTHETIC-BATCH-KEY-9f3c2a1b\n"));
    assert!(secret.contains("ClientIp=203.0.113.10\n"));
    assert!(secret.contains("SLD=example\n"));
    assert!(secret.contains("TLD=test\n"));

    // Must not echo ApiKey on stdout/stderr.
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("SYNTHETIC-BATCH-KEY-9f3c2a1b"),
        "secret leaked to terminal output"
    );
}

#[test]
fn batch_namecheap_domain_flag_splits_sld_tld() {
    let staging = temp_dir("nc-domain");
    let secret_file = staging.join("api-key.txt");
    fs::write(&secret_file, "SYNTHETIC-DOMAIN-KEY-aa11bb22").unwrap();

    let out = Command::new(bin())
        .args([
            "namecheap-api",
            "--host",
            "surmount-1",
            "--batch",
            "--staging",
            staging.to_str().unwrap(),
            "--api-user",
            "labuser",
            "--client-ip",
            "203.0.113.10",
            "--domain",
            "example.co.uk",
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let secret = fs::read_to_string(staging.join("namecheap-api").join("secret")).unwrap();
    assert!(secret.contains("SLD=example\n"));
    assert!(secret.contains("TLD=co.uk\n"));
    assert!(secret.contains("ApiKey=SYNTHETIC-DOMAIN-KEY-aa11bb22\n"));

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("SYNTHETIC-DOMAIN-KEY-aa11bb22"),
        "secret leaked to terminal output"
    );
}

#[test]
fn batch_shc_api_staging_shape() {
    let staging = temp_dir("shc");
    let secret_file = staging.join("api-key.txt");
    fs::write(&secret_file, "shc_live_SYNTHETIC-BATCH-KEY-cc33dd44").unwrap();

    let out = Command::new(bin())
        .args([
            "shc-api",
            "--host",
            "surmount-1",
            "--batch",
            "--staging",
            staging.to_str().unwrap(),
            "--api-base",
            "https://blesta.sovereignhybridcompute.com/user-api/v2",
            "--service-id",
            "12345",
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let item = staging.join("shc-api");
    let attrs = fs::read_to_string(item.join("attributes")).unwrap();
    let secret = fs::read_to_string(item.join("secret")).unwrap();

    assert!(attrs.contains("surmount.kind=shc-api\n"));
    assert!(attrs.contains("surmount.host=surmount-1\n"));
    assert!(attrs.contains("surmount.path=/var/lib/surmount/secrets/rdns/shc.env\n"));
    assert!(secret.contains("ApiKey=shc_live_SYNTHETIC-BATCH-KEY-cc33dd44\n"));
    assert!(secret.contains("ApiBase=https://blesta.sovereignhybridcompute.com/user-api/v2\n"));
    assert!(secret.contains("ServiceId=12345\n"));

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("shc_live_SYNTHETIC-BATCH-KEY-cc33dd44"),
        "secret leaked to terminal output"
    );
}

#[test]
fn batch_shc_api_generate_refused() {
    let staging = temp_dir("shc-gen");
    let out = Command::new(bin())
        .args([
            "shc-api",
            "--host",
            "surmount-1",
            "--batch",
            "--generate",
            "--staging",
            staging.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        !out.status.success(),
        "generate must be refused for shc-api"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("generate") && (err.contains("refused") || err.contains("not")),
        "stderr should explain generate refuse: {err}"
    );
    assert!(!staging.join("shc-api").join("secret").exists());
}

#[test]
fn batch_shc_api_default_base_no_service_id() {
    let staging = temp_dir("shc-def");
    let secret_file = staging.join("api-key.txt");
    fs::write(&secret_file, "shc_live_SYNTHETIC-DEFAULT-BASE-ee55").unwrap();

    let out = Command::new(bin())
        .args([
            "shc-api",
            "--host",
            "surmount-1",
            "--batch",
            "--staging",
            staging.to_str().unwrap(),
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let secret = fs::read_to_string(staging.join("shc-api").join("secret")).unwrap();
    assert!(secret.contains("ApiBase=https://blesta.sovereignhybridcompute.com/user-api/v2\n"));
    assert!(!secret.contains("ServiceId="));
}

#[test]
fn batch_empty_secret_fail_closed() {
    let staging = temp_dir("empty");
    let secret_file = staging.join("empty.txt");
    fs::write(&secret_file, "").unwrap();

    let out = Command::new(bin())
        .args([
            "stalwart-token",
            "--host",
            "mail-lab",
            "--batch",
            "--staging",
            staging.to_str().unwrap(),
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(!out.status.success(), "empty secret must fail closed");
    assert!(!staging.join("stalwart-token").join("secret").exists());
}

#[test]
fn batch_stalwart_token_staging_shape() {
    let staging = temp_dir("st-ok");
    let secret_file = staging.join("token.txt");
    fs::write(&secret_file, "SYNTHETIC-STALWART-BATCH-TOKEN-aa11").unwrap();

    let out = Command::new(bin())
        .args([
            "stalwart-token",
            "--host",
            "surmount-1",
            "--batch",
            "--staging",
            staging.to_str().unwrap(),
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let item = staging.join("stalwart-token");
    let attrs = fs::read_to_string(item.join("attributes")).unwrap();
    let secret = fs::read_to_string(item.join("secret")).unwrap();
    assert!(attrs.contains("surmount.kind=stalwart-token\n"));
    assert!(attrs.contains("surmount.host=surmount-1\n"));
    assert!(attrs.contains("surmount.path=/var/lib/surmount/secrets/ui/stalwart-api-token\n"));
    assert_eq!(secret, "SYNTHETIC-STALWART-BATCH-TOKEN-aa11");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains("SYNTHETIC-STALWART-BATCH-TOKEN-aa11"),
        "secret leaked to terminal output"
    );
    // Batch mode must not dump interactive next-steps noise (still OK if present,
    // but must never include the secret). Metadata write line is fine.
    assert!(combined.contains("wrote staging") || combined.contains("stalwart-token"));
}

#[test]
fn batch_stalwart_token_generate_refused() {
    let staging = temp_dir("st-gen");
    let out = Command::new(bin())
        .args([
            "stalwart-token",
            "--host",
            "surmount-1",
            "--batch",
            "--generate",
            "--staging",
            staging.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        !out.status.success(),
        "generate must be refused for stalwart-token"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("generate") && (err.contains("refused") || err.contains("not")),
        "stderr should explain generate refuse: {err}"
    );
    assert!(
        err.contains("engine") || err.contains("accepts") || err.contains("registration"),
        "stderr should mention engine registration honesty: {err}"
    );
    assert!(!staging.join("stalwart-token").join("secret").exists());
}

#[test]
fn batch_synology_afp_generate_refused() {
    let staging = temp_dir("afp-gen");
    let out = Command::new(bin())
        .args([
            "synology-afp",
            "--host",
            "DS1513",
            "--batch",
            "--generate",
            "--staging",
            staging.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        !out.status.success(),
        "generate must be refused for synology-afp"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("generate") && (err.contains("refused") || err.contains("not")),
        "stderr should explain generate refuse: {err}"
    );
    assert!(
        err.contains("synology-afp") || err.contains("LAN") || err.contains("password"),
        "stderr should mention this kind / LAN password: {err}"
    );
    assert!(!staging.join("synology-afp").join("secret").exists());
}

#[test]
fn batch_synology_afp_legacy_diskstation_refused() {
    let staging = temp_dir("afp-legacy");
    let out = Command::new(bin())
        .args([
            "synology-afp",
            "--host",
            "diskstation",
            "--batch",
            "--secret-service",
            "--no-staging",
            "--staging",
            staging.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        !out.status.success(),
        "legacy diskstation host must be refused"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("DS1513") && err.contains("DS3018xs"),
        "stderr should name both canonical host ids: {err}"
    );
}

#[test]
fn batch_synology_afp_requires_secret_service() {
    let staging = temp_dir("afp-ss");
    let out = Command::new(bin())
        .args([
            "synology-afp",
            "--host",
            "DS3018xs",
            "--batch",
            "--staging",
            staging.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        !out.status.success(),
        "synology-afp without --secret-service must fail"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("secret-service") || err.contains("Secret Service"),
        "stderr should require --secret-service: {err}"
    );
    assert!(!staging.join("synology-afp").join("secret").exists());
}

#[test]
fn batch_synology_afp_stores_gnome_network_password() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("afp-np");
    let secret_file = dir.join("pass.txt");
    let synth = "SYNTHETIC-AFP-LAN-not-real-aa11";
    fs::write(&secret_file, synth).unwrap();

    let st_log = dir.join("secret-tool.log");
    let st_stdin = dir.join("secret-tool.stdin");
    let st = dir.join("secret-tool");
    fs::write(
        &st,
        format!(
            "#!/bin/sh\n\
             set -eu\n\
             printf '%s\\n' \"$*\" >>{log}\n\
             case \" $* \" in *\" {synth} \"*) echo 'secret-tool mock: password must never appear on argv' >&2; exit 3 ;; esac\n\
             case \"${{1:-}}\" in store)\n\
               cat >>{stdin} || true\n\
               printf '\\n' >>{stdin}\n\
               exit 0\n\
             ;; esac\n\
             echo \"secret-tool mock: unexpected $*\" >&2\n\
             exit 2\n",
            log = st_log.display(),
            stdin = st_stdin.display(),
            synth = synth,
        ),
    )
    .unwrap();
    let mut perm = fs::metadata(&st).unwrap().permissions();
    perm.set_mode(0o700);
    fs::set_permissions(&st, perm).unwrap();

    let testnet = "192.0.2.10";
    let out = Command::new(bin())
        .env("SURMOUNT_SECRET_TOOL", &st)
        .env("PATH", path_with_wipe_stubs(&dir))
        .args([
            "synology-afp",
            "--host",
            "DS1513",
            "--afp-host",
            testnet,
            "--batch",
            "--secret-service",
            "--no-staging",
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let log = fs::read_to_string(&st_log).unwrap_or_default();
    assert!(
        log.contains("surmount.kind") && log.contains("synology-afp") && log.contains("DS1513"),
        "must still store surmount.kind synology-afp host DS1513: {log}"
    );
    assert!(
        log.contains("org.gnome.keyring.NetworkPassword"),
        "must store GNOME NetworkPassword schema: {log}"
    );
    assert!(
        log.contains("protocol") && log.contains("afp"),
        "NetworkPassword protocol must be afp: {log}"
    );
    assert!(
        !log.contains(&format!("server {testnet}")),
        "NetworkPassword server must not be IPv4 even when --afp-host is IPv4: {log}"
    );
    assert!(
        log.contains("server DS1513"),
        "NetworkPassword server must be host id DS1513: {log}"
    );
    assert!(
        log.contains("DS1513.local"),
        "also remember <host>.local: {log}"
    );
    assert!(
        log.contains("user") && log.contains("hunter"),
        "NetworkPassword user must be hunter: {log}"
    );
    assert!(
        !log.contains(synth),
        "password must never appear on secret-tool argv"
    );

    let stdin = fs::read_to_string(&st_stdin).unwrap_or_default();
    assert!(
        stdin.contains(synth),
        "password must be on secret-tool stdin, not argv"
    );

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains(synth),
        "secret leaked to terminal output"
    );
}

#[test]
fn batch_synology_afp_host_only_stores_mdns_network_password() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("afp-np-mdns");
    let secret_file = dir.join("pass.txt");
    let synth = "SYNTHETIC-AFP-LAN-not-real-bb22";
    fs::write(&secret_file, synth).unwrap();

    let st_log = dir.join("secret-tool.log");
    let st = dir.join("secret-tool");
    fs::write(
        &st,
        format!(
            "#!/bin/sh\n\
             set -eu\n\
             printf '%s\\n' \"$*\" >>{log}\n\
             case \" $* \" in *\" {synth} \"*) exit 3 ;; esac\n\
             case \"${{1:-}}\" in store) cat >/dev/null; exit 0 ;; esac\n\
             exit 2\n",
            log = st_log.display(),
            synth = synth,
        ),
    )
    .unwrap();
    let mut perm = fs::metadata(&st).unwrap().permissions();
    perm.set_mode(0o700);
    fs::set_permissions(&st, perm).unwrap();

    let out = Command::new(bin())
        .env("SURMOUNT_SECRET_TOOL", &st)
        .env("PATH", path_with_wipe_stubs(&dir))
        .args([
            "synology-afp",
            "--host",
            "DS3018xs",
            "--batch",
            "--secret-service",
            "--no-staging",
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let log = fs::read_to_string(&st_log).unwrap_or_default();
    assert!(
        log.contains("synology-afp") && log.contains("DS3018xs"),
        "must store surmount synology-afp DS3018xs: {log}"
    );
    assert!(
        log.contains("org.gnome.keyring.NetworkPassword") && log.contains("DS3018xs.local"),
        "host-only intake should remember mDNS URI form: {log}"
    );
    assert!(
        log.contains("server DS3018xs"),
        "host-only intake should also remember host id: {log}"
    );
    assert!(
        !log.contains(synth),
        "password must never appear on secret-tool argv"
    );
}

#[test]
fn batch_session_generate() {
    let staging = temp_dir("sess");
    let out = Command::new(bin())
        .args([
            "session-secret",
            "--host",
            "mail-lab",
            "--batch",
            "--generate",
            "--staging",
            staging.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let secret = fs::read_to_string(staging.join("session-secret").join("secret")).unwrap();
    assert_eq!(secret.len(), 64);
    assert!(secret.chars().all(|c| c.is_ascii_hexdigit()));
}

/// After any secret-tool store, wipe clipboard / PRIMARY / wl-copy.
/// Spies on PATH; no live X. Never pipe the secret into the wipe tools.
#[test]
fn batch_store_wipes_paste_buffers() {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("wipe-clip");
    let secret_file = dir.join("pass.txt");
    let synth = "SYNTHETIC-WIPE-SESSION-not-real-cc33";
    fs::write(&secret_file, synth).unwrap();

    let st_log = dir.join("secret-tool.log");
    let st = dir.join("secret-tool");
    fs::write(
        &st,
        format!(
            "#!/bin/sh\n\
             set -eu\n\
             printf '%s\\n' \"$*\" >>{log}\n\
             case \" $* \" in *\" {synth} \"*) exit 3 ;; esac\n\
             case \"${{1:-}}\" in store) cat >/dev/null; exit 0 ;; esac\n\
             exit 2\n",
            log = st_log.display(),
            synth = synth,
        ),
    )
    .unwrap();
    let mut perm = fs::metadata(&st).unwrap().permissions();
    perm.set_mode(0o700);
    fs::set_permissions(&st, perm.clone()).unwrap();

    let bin_dir = dir.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let xclip_log = dir.join("xclip.log");
    let xclip_stdin = dir.join("xclip.stdin");
    let pbcopy_log = dir.join("pbcopy.log");
    let pbcopy_stdin = dir.join("pbcopy.stdin");
    let wl_log = dir.join("wl-copy.log");

    let xclip = bin_dir.join("xclip");
    fs::write(
        &xclip,
        format!(
            "#!/bin/sh\n\
             set -eu\n\
             printf 'argv=%s\\n' \"$*\" >>{log}\n\
             if [ ! -t 0 ]; then\n\
               cat >>{stdin} || true\n\
             fi\n\
             exit 0\n",
            log = xclip_log.display(),
            stdin = xclip_stdin.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&xclip, perm.clone()).unwrap();

    let pbcopy = bin_dir.join("pbcopy");
    fs::write(
        &pbcopy,
        format!(
            "#!/bin/sh\n\
             set -eu\n\
             printf 'argv=%s\\n' \"$*\" >>{log}\n\
             if [ ! -t 0 ]; then\n\
               cat >>{stdin} || true\n\
             fi\n\
             exit 0\n",
            log = pbcopy_log.display(),
            stdin = pbcopy_stdin.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&pbcopy, perm.clone()).unwrap();

    let wl = bin_dir.join("wl-copy");
    fs::write(
        &wl,
        format!(
            "#!/bin/sh\n\
             set -eu\n\
             printf 'argv=%s\\n' \"$*\" >>{log}\n\
             exit 0\n",
            log = wl_log.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&wl, perm).unwrap();

    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(bin())
        .env("PATH", &path)
        .env("SURMOUNT_SECRET_TOOL", &st)
        .args([
            "session-secret",
            "--host",
            "mail-lab",
            "--batch",
            "--secret-service",
            "--no-staging",
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "store must succeed; wipe is best-effort. stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let xclip_args = fs::read_to_string(&xclip_log).unwrap_or_default();
    assert!(
        xclip_args.contains("-selection clipboard") || xclip_args.contains("clipboard"),
        "after store, wipe clipboard via xclip: {xclip_args}"
    );
    assert!(
        xclip_args.contains("-selection primary") || xclip_args.contains("primary"),
        "after store, wipe PRIMARY via xclip: {xclip_args}"
    );
    let x_in = fs::read_to_string(&xclip_stdin).unwrap_or_default();
    assert!(
        !x_in.contains(synth),
        "wipe must never pipe the secret into xclip"
    );
    assert!(
        x_in.trim().is_empty(),
        "xclip wipe stdin must be empty, got {x_in:?}"
    );

    let pb_args = fs::read_to_string(&pbcopy_log).unwrap_or_default();
    assert!(
        !pb_args.is_empty(),
        "after store, wipe via pbcopy (empty stdin): {pb_args}"
    );
    let pb_in = fs::read_to_string(&pbcopy_stdin).unwrap_or_default();
    assert!(
        !pb_in.contains(synth),
        "wipe must never pipe the secret into pbcopy"
    );
    assert!(
        pb_in.trim().is_empty(),
        "pbcopy wipe stdin must be empty, got {pb_in:?}"
    );

    let wl_args = fs::read_to_string(&wl_log).unwrap_or_default();
    assert!(
        wl_args.contains("--clear"),
        "after store, wl-copy --clear: {wl_args}"
    );

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !combined.contains(synth),
        "secret leaked to terminal output"
    );
}
