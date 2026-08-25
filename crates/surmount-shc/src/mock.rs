use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use crate::die;
use crate::jsonutil::json_string_field;

const DEFAULT_RDNS_STATE: &str =
    r#"{"data":{"service_id":12345,"records":[{"ip":"203.0.113.50","ptr":null,"pending":null}]}}"#;
const DEFAULT_VM: &str = r#"{"data":[{"service_id":12345,"label":"mail-lab","status":"active"}]}"#;
const DEFAULT_DEPARTMENTS: &str =
    r#"{"data":[{"id":1,"name":"SHC Team","description":"Internal Support"}]}"#;

#[derive(Debug, Clone)]
pub struct MockHttp {
    pub dir: PathBuf,
}

impl MockHttp {
    pub fn ensure(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).map_err(|e| die(format!("mock dir: {e}")))?;
        let state = dir.join("rdns_state.json");
        if !state.exists() {
            fs::write(&state, DEFAULT_RDNS_STATE)
                .map_err(|e| die(format!("mock rdns_state: {e}")))?;
        }
        let vm = dir.join("vm.json");
        if !vm.exists() {
            fs::write(&vm, DEFAULT_VM).map_err(|e| die(format!("mock vm.json: {e}")))?;
        }
        let log = dir.join("requests.log");
        if !log.exists() {
            fs::write(&log, "").map_err(|e| die(format!("mock requests.log: {e}")))?;
        }
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    pub fn request(
        &self,
        method: &str,
        path: &str,
        body: &str,
        confirm: Option<&str>,
    ) -> Result<(u16, String)> {
        let mut line = format!("{method} {path}");
        if let Some(c) = confirm {
            line.push_str(&format!(" confirm={c}"));
        }
        line.push('\n');
        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("requests.log"))
            .map_err(|e| die(format!("mock log: {e}")))?;
        log.write_all(line.as_bytes())
            .map_err(|e| die(format!("mock log: {e}")))?;

        if !body.is_empty() && (method == "POST" || method == "DELETE") {
            fs::write(self.dir.join("last_write_body"), body)
                .map_err(|e| die(format!("mock last_write_body: {e}")))?;
        }

        let sid = service_id_from_path(path);

        match (method, path) {
            ("GET", "/vm" | "/vm/") => {
                let body = fs::read_to_string(self.dir.join("vm.json"))
                    .unwrap_or_else(|_| DEFAULT_VM.to_string());
                return Ok((200, body));
            }
            ("GET", "/support/departments" | "/support/departments/") => {
                let dept_path = self.dir.join("departments.json");
                let body = if dept_path.is_file() {
                    fs::read_to_string(dept_path)
                        .unwrap_or_else(|_| DEFAULT_DEPARTMENTS.to_string())
                } else {
                    DEFAULT_DEPARTMENTS.to_string()
                };
                return Ok((200, body));
            }
            _ => {}
        }

        if method == "GET" && is_rdns_path(path) {
            let named = self.dir.join(format!("rdns.{sid}.json"));
            let body = if named.is_file() {
                fs::read_to_string(named).unwrap_or_else(|_| DEFAULT_RDNS_STATE.to_string())
            } else {
                fs::read_to_string(self.dir.join("rdns_state.json"))
                    .unwrap_or_else(|_| DEFAULT_RDNS_STATE.to_string())
            };
            return Ok((200, body));
        }

        if method == "POST" && is_rdns_path(path) {
            if confirm.is_none() {
                return self.confirmation_required("mock-confirm-", body);
            }
            let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let ip = json_string_field(&parsed, &["ip"]);
            let host = json_string_field(&parsed, &["hostname"]);
            if ip.is_empty() {
                return Err(die("mock: POST body missing ip"));
            }
            let sid_num: u64 = sid.parse().unwrap_or(12345);
            let state = json!({
                "data": {
                    "service_id": sid_num,
                    "records": [{ "ip": ip, "ptr": host, "pending": Value::Null }]
                }
            });
            fs::write(
                self.dir.join("rdns_state.json"),
                serde_json::to_string(&state).map_err(|e| die(format!("mock state json: {e}")))?,
            )
            .map_err(|e| die(format!("mock state: {e}")))?;
            let resp = json!({
                "data": {
                    "service_id": sid_num,
                    "ip": ip,
                    "hostname": host,
                    "status": "queued",
                    "job_id": 1
                }
            });
            return Ok((202, resp.to_string()));
        }

        if method == "POST" && (path == "/support/tickets" || path == "/support/tickets/") {
            if confirm.is_none() {
                return self.confirmation_required("mock-confirm-ticket-", body);
            }
            let resp = json!({
                "data": {
                    "ticket_id": 9001,
                    "department_id": 1,
                    "subject": "Cannot reach lab VM",
                    "status": "open"
                }
            });
            return Ok((201, resp.to_string()));
        }

        if method == "DELETE" && is_rdns_path(path) {
            if confirm.is_none() {
                return self.confirmation_required("mock-confirm-del-", body);
            }
            let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let ip = json_string_field(&parsed, &["ip"]);
            let sid_num: u64 = sid.parse().unwrap_or(12345);
            let state = json!({
                "data": {
                    "service_id": sid_num,
                    "records": [{ "ip": ip, "ptr": Value::Null, "pending": Value::Null }]
                }
            });
            fs::write(
                self.dir.join("rdns_state.json"),
                serde_json::to_string(&state).map_err(|e| die(format!("mock state json: {e}")))?,
            )
            .map_err(|e| die(format!("mock state: {e}")))?;
            let resp = json!({
                "data": {
                    "service_id": sid_num,
                    "ip": ip,
                    "status": "queued",
                    "job_id": 2
                }
            });
            return Ok((202, resp.to_string()));
        }

        Err(die(format!("mock: unhandled {method} {path}")))
    }

    fn confirmation_required(&self, prefix: &str, body: &str) -> Result<(u16, String)> {
        let cid = format!("{prefix}{}", mock_cksum(body));
        fs::write(self.dir.join("last_confirm_id"), &cid)
            .map_err(|e| die(format!("mock confirm id: {e}")))?;
        let resp = json!({
            "error": {
                "code": "confirmation_required",
                "message": "Confirmation required"
            },
            "confirmation": { "confirmation_id": cid }
        });
        Ok((409, resp.to_string()))
    }
}

fn is_rdns_path(path: &str) -> bool {
    // /vm/{digits}/rdns
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    parts.len() == 3 && parts[0] == "vm" && parts[2] == "rdns" && !parts[1].is_empty()
}

fn service_id_from_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 && parts[0] == "vm" {
        parts[1].to_string()
    } else {
        "12345".to_string()
    }
}

fn mock_cksum(body: &str) -> u32 {
    // Deterministic stand-in for POSIX cksum (mock ids are not a secret).
    let mut crc: u32 = 0;
    for b in body.as_bytes() {
        crc = crc.wrapping_add(u32::from(*b));
        crc = crc.rotate_left(3) ^ 0x04C1_1DB7;
    }
    crc
}
