//! Hermetic CLI tests ported from script/test-rdns-shc.sh.
//! Mock API via SURMOUNT_RDNS_SHC_MOCK_DIR. No network, no live SHC, no real keys.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const SYNTHETIC_KEY: &str = "shc_live_SYNTHETIC-hermetic-not-real-001";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-shc")
}

fn scratch(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-shc-cli-{}-{}-{}",
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

struct Harness {
    root: PathBuf,
    mock: PathBuf,
    cred: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let root = scratch("h");
        let mock = root.join("mock");
        fs::create_dir_all(&mock).unwrap();
        let cred = root.join("shc.env");
        fs::write(
            &cred,
            format!(
                "# synthetic hermetic fixture; not a real SHC key\n\
                 ApiKey={SYNTHETIC_KEY}\n\
                 ApiBase=https://blesta.sovereignhybridcompute.com/user-api/v2\n\
                 ServiceId=12345\n"
            ),
        )
        .unwrap();
        Self { root, mock, cred }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(bin());
        c.env("SURMOUNT_RDNS_SHC_ENV", &self.cred);
        c.env("SURMOUNT_RDNS_SHC_MOCK_DIR", &self.mock);
        c.env("SURMOUNT_RDNS_SHC_VERBOSE", "0");
        c.env_remove("SURMOUNT_RDNS_SHC_API_BASE");
        c
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        let out = self.cmd().args(args).output().expect("run surmount-shc");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let code = out.status.code().unwrap_or(1);
        (code, combined)
    }

    fn truncate_log(&self) {
        fs::write(self.mock.join("requests.log"), "").unwrap();
    }

    fn log_text(&self) -> String {
        fs::read_to_string(self.mock.join("requests.log")).unwrap_or_default()
    }
}

fn no_key(s: &str) {
    assert!(!s.contains(SYNTHETIC_KEY), "ApiKey leaked in output: {s}");
}

#[test]
fn usage_without_args_exits_non_zero() {
    let h = Harness::new();
    let (rc, _) = h.run(&[]);
    assert_ne!(rc, 0, "no-args should exit non-zero");
}

#[test]
fn help_documents_defaults_confirm_and_apikey_safety() {
    let h = Harness::new();
    let (rc, out) = h.run(&["--help"]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("mail.surmount.systems"), "{out}");
    assert!(out.to_ascii_lowercase().contains("confirm"), "{out}");
    assert!(out.contains("ApiKey"), "{out}");
}

#[test]
fn help_documents_ticket_commands() {
    let h = Harness::new();
    let (rc, out) = h.run(&["--help"]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("--list-departments"), "{out}");
    assert!(out.contains("--open-ticket"), "{out}");
    assert!(out.contains("department_id"), "{out}");
}

#[test]
fn list_shows_mock_rdns_without_leaking_apikey() {
    let h = Harness::new();
    let (rc, out) = h.run(&["--list"]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("203.0.113.50"), "{out}");
    no_key(&out);
}

#[test]
fn list_vms_shows_mock_service_ids() {
    let h = Harness::new();
    let (rc, out) = h.run(&["--list-vms"]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("12345"), "{out}");
    no_key(&out);
}

#[test]
fn extra_json_fields_on_list_are_ignored() {
    let h = Harness::new();
    fs::write(
        h.mock.join("rdns_state.json"),
        r#"{"data":{"service_id":12345,"vendor_extra":true,"records":[{"ip":"203.0.113.50","ptr":null,"pending":null,"mysterious":{"a":1}}]}}"#,
    )
    .unwrap();
    let (rc, out) = h.run(&["--list"]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("203.0.113.50"), "{out}");
}

#[test]
fn dry_run_set_does_not_post() {
    let h = Harness::new();
    h.truncate_log();
    let (rc, out) = h.run(&[
        "--ip",
        "203.0.113.50",
        "--hostname",
        "mail.surmount.systems",
    ]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.to_ascii_lowercase().contains("dry-run"), "{out}");
    let log = h.log_text();
    assert!(
        !log.lines().any(|l| l.starts_with("POST")),
        "dry-run set should not POST: {log}"
    );
}

#[test]
fn live_set_confirm_dance_then_list_shows_ptr() {
    let h = Harness::new();
    h.truncate_log();
    let (rc, out) = h.run(&[
        "--live",
        "--ip",
        "203.0.113.50",
        "--hostname",
        "mail.surmount.systems",
    ]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.to_ascii_lowercase().contains("queued"), "{out}");
    let log = h.log_text();
    assert!(log.contains("POST /vm/12345/rdns"), "missing POST: {log}");
    assert!(log.contains("confirm="), "missing confirm dance: {log}");
    no_key(&out);

    let (rc, listed) = h.run(&["--list"]);
    assert_eq!(rc, 0, "{listed}");
    assert!(listed.contains("mail.surmount.systems"), "{listed}");
}

#[test]
fn live_clear_confirm_dance() {
    let h = Harness::new();
    let _ = h.run(&[
        "--live",
        "--ip",
        "203.0.113.50",
        "--hostname",
        "mail.surmount.systems",
    ]);
    h.truncate_log();
    let (rc, out) = h.run(&["--live", "--clear", "--ip", "203.0.113.50"]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.to_ascii_lowercase().contains("queued"), "{out}");
    let log = h.log_text();
    assert!(
        log.contains("DELETE /vm/12345/rdns"),
        "missing DELETE: {log}"
    );
    assert!(log.contains("confirm="), "missing confirm dance: {log}");
}

#[test]
fn missing_credentials_fails_closed() {
    let h = Harness::new();
    let missing = h.root.join("missing.env");
    let out = h
        .cmd()
        .env("SURMOUNT_RDNS_SHC_ENV", &missing)
        .args(["--list"])
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{combined}");
    let lower = combined.to_ascii_lowercase();
    assert!(
        lower.contains("missing") || lower.contains("credentials"),
        "{combined}"
    );
}

#[test]
fn list_departments_shows_mock_shc_team() {
    let h = Harness::new();
    fs::write(
        h.mock.join("departments.json"),
        r#"{"data":[{"id":1,"name":"SHC Team","description":"Internal Support","vendor_extra":false}]}"#,
    )
    .unwrap();
    let (rc, out) = h.run(&["--list-departments"]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("SHC Team"), "{out}");
    assert!(out.contains('1'), "{out}");
    no_key(&out);
}

#[test]
fn dry_run_open_ticket_does_not_post() {
    let h = Harness::new();
    h.truncate_log();
    let (rc, out) = h.run(&[
        "--open-ticket",
        "--department-id",
        "1",
        "--subject",
        "Cannot reach lab VM",
        "--message",
        "Synthetic hermetic details only.",
    ]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.to_ascii_lowercase().contains("dry-run"), "{out}");
    let log = h.log_text();
    assert!(
        !log.contains("POST /support/tickets"),
        "dry-run open-ticket should not POST: {log}"
    );
}

#[test]
fn live_open_ticket_confirm_dance_prints_ticket_id() {
    let h = Harness::new();
    h.truncate_log();
    let (rc, out) = h.run(&[
        "--live",
        "--open-ticket",
        "--department-id",
        "1",
        "--subject",
        "Cannot reach lab VM",
        "--message",
        "Synthetic hermetic details only.",
    ]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("ticket_id=9001"), "{out}");
    assert!(out.contains("department_id=1"), "{out}");
    let log = h.log_text();
    assert!(log.contains("POST /support/tickets"), "missing POST: {log}");
    assert!(log.contains("confirm="), "missing confirm dance: {log}");
    no_key(&out);
}

#[test]
fn live_open_ticket_priority_and_message_file() {
    let h = Harness::new();
    let msg = h.root.join("ticket.txt");
    fs::write(&msg, "Synthetic hermetic details only.\n").unwrap();
    h.truncate_log();
    let (rc, out) = h.run(&[
        "--live",
        "--open-ticket",
        "--department-id",
        "1",
        "--subject",
        "Cannot reach lab VM",
        "--message-file",
        msg.to_str().unwrap(),
        "--priority",
        "emergency",
    ]);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("ticket_id=9001"), "{out}");
    let body = fs::read_to_string(h.mock.join("last_write_body")).unwrap();
    assert!(body.contains("\"priority\":\"emergency\""), "{body}");
    assert!(body.contains("\"department_id\":1"), "{body}");
    assert!(!body.contains(SYNTHETIC_KEY), "{body}");
}

#[test]
fn invalid_priority_fails() {
    let h = Harness::new();
    let (rc, out) = h.run(&[
        "--open-ticket",
        "--department-id",
        "1",
        "--subject",
        "x",
        "--message",
        "y",
        "--priority",
        "urgent",
    ]);
    assert_ne!(rc, 0, "{out}");
    assert!(out.contains("emergency"), "{out}");
}

#[test]
fn never_print_apikey_on_successful_paths() {
    let h = Harness::new();
    let mut combined = String::new();
    for args in [
        vec!["--list"],
        vec!["--list-vms"],
        vec!["--ip", "203.0.113.50"],
        vec!["--list-departments"],
    ] {
        let (rc, out) = h.run(&args);
        assert_eq!(rc, 0, "{out}");
        combined.push_str(&out);
    }
    no_key(&combined);
}
