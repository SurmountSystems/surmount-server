//! SHC customer user-api client: reverse DNS (PTR) and support tickets.
//!
//! Credentials: KEY=value env body (`ApiKey=`, optional `ApiBase=`, `ServiceId=`).
//! Never log or print ApiKey. Mock mode is `SURMOUNT_RDNS_SHC_MOCK_DIR` (no network).
//! JSON parse is additionalProperties-safe (unknown fields ignored).
//!
//! Docs: docs/DNS.md, docs/OPS.md, docs/SECRETS.md
//! KB: https://blesta.sovereignhybridcompute.com/plugin/support_manager/knowledgebase/view/16/the-customer-api-and-mcp/
//! OpenAPI: https://blesta.sovereignhybridcompute.com/user-api/openapi.json
//! (accessed: 2026-08-11)

mod api;
mod cred;
mod jsonutil;
mod mock;
mod run;

pub use cred::{
    Credentials, DEFAULT_API_BASE, DEFAULT_CRED_PATH, DEFAULT_HOSTNAME, resolve_cred_path,
};
pub use jsonutil::{
    TicketPriority, extract_confirmation_id, is_confirmation_required, parse_ipv4,
    ticket_request_body,
};
pub use run::{Cli, run};

fn die(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!("rdns-shc: {msg}")
}

fn redact(text: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        return text.to_string();
    }
    text.replace(api_key, "<redacted>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "surmount-shc-unit-{}-{}-{}",
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

    #[test]
    fn extra_json_fields_do_not_break_confirmation_parse() {
        let body = json!({
            "error": {
                "code": "confirmation_required",
                "message": "Confirmation required",
                "extra_vendor_field": true
            },
            "confirmation": {
                "confirmation_id": "cid-extra",
                "ttl": 30,
                "nested": {"foo": 1}
            },
            "unexpected": [1, 2, 3]
        });
        let s = body.to_string();
        assert!(is_confirmation_required(&s));
        assert_eq!(extract_confirmation_id(&s).as_deref(), Some("cid-extra"));
    }

    #[test]
    fn ticket_json_encodes_priority_and_numeric_department() {
        let raw = ticket_request_body(
            "1",
            "Cannot reach lab VM",
            "Synthetic hermetic details only.",
            Some(TicketPriority::Emergency),
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["department_id"], 1);
        assert_eq!(v["subject"], "Cannot reach lab VM");
        assert_eq!(v["message"], "Synthetic hermetic details only.");
        assert_eq!(v["priority"], "emergency");
        let obj = v.as_object().unwrap();
        assert_eq!(obj.len(), 4);
    }

    #[test]
    fn ticket_json_omits_priority_when_absent() {
        let raw = ticket_request_body("12", "subj", "msg", None).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(v.get("priority").is_none());
        assert_eq!(v["department_id"], 12);
    }

    #[test]
    fn invalid_ipv4_rejected() {
        assert!(parse_ipv4("203.0.113.50").is_ok());
        assert!(parse_ipv4("not-an-ip").is_err());
        assert!(parse_ipv4("203.0.113").is_err());
    }

    #[test]
    fn ticket_priority_parses_five_levels() {
        assert_eq!(
            TicketPriority::parse("emergency").unwrap(),
            TicketPriority::Emergency
        );
        assert_eq!(
            TicketPriority::parse("critical").unwrap(),
            TicketPriority::Critical
        );
        assert_eq!(TicketPriority::parse("high").unwrap(), TicketPriority::High);
        assert_eq!(
            TicketPriority::parse("medium").unwrap(),
            TicketPriority::Medium
        );
        assert_eq!(TicketPriority::parse("low").unwrap(), TicketPriority::Low);
        assert!(TicketPriority::parse("urgent").is_err());
    }

    #[test]
    fn credentials_refuse_symlink_and_require_apikey() {
        let dir = scratch("cred");
        let real = dir.join("shc.env");
        fs::write(&real, "ApiKey=shc_live_SYNTHETIC-hermetic-not-real-001\n").unwrap();
        let link = dir.join("link.env");
        symlink(&real, &link).unwrap();
        let err = Credentials::load(&link, None, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("symlink"), "{err}");
        assert!(!err.contains("shc_live_SYNTHETIC"));
    }

    #[test]
    fn credentials_ignore_unknown_keys() {
        let dir = scratch("cred-ok");
        let path = dir.join("shc.env");
        fs::write(
            &path,
            "ApiKey=shc_live_SYNTHETIC-hermetic-not-real-001\n\
             ApiBase=https://example.test/user-api/v2/\n\
             ServiceId=12345\n\
             ExtraVendor=ignored\n",
        )
        .unwrap();
        let c = Credentials::load(&path, None, Some("999")).unwrap();
        assert_eq!(c.api_base, "https://example.test/user-api/v2");
        assert_eq!(c.service_id.as_deref(), Some("999"));
        assert_eq!(c.api_key, "shc_live_SYNTHETIC-hermetic-not-real-001");
    }

    #[test]
    fn redact_strips_api_key() {
        let key = "shc_live_SYNTHETIC-hermetic-not-real-001";
        let leaked = format!("Bearer {key} oops");
        assert_eq!(redact(&leaked, key), "Bearer <redacted> oops");
    }

    #[test]
    fn crate_sources_do_not_invoke_python() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let needle = format!("{}{}", "python", "3");
        let mut hits = Vec::new();
        walk_needle(&root, &needle, &mut hits);
        assert!(
            hits.is_empty(),
            "interpreter must not appear in crate sources: {hits:?}"
        );
    }

    fn walk_needle(dir: &std::path::Path, needle: &str, hits: &mut Vec<PathBuf>) {
        for ent in fs::read_dir(dir).unwrap() {
            let ent = ent.unwrap();
            let path = ent.path();
            if path.is_dir() {
                walk_needle(&path, needle, hits);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            if text.contains(needle) {
                hits.push(path);
            }
        }
    }
}
