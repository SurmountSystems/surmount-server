use anyhow::Result;
use serde_json::{Value, json};

use crate::die;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketPriority {
    Emergency,
    Critical,
    High,
    Medium,
    Low,
}

impl TicketPriority {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "emergency" => Ok(Self::Emergency),
            "critical" => Ok(Self::Critical),
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "low" => Ok(Self::Low),
            _ => Err(die(
                "--priority must be emergency, critical, high, medium, or low",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Emergency => "emergency",
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

pub fn parse_ipv4(ip: &str) -> Result<()> {
    let ok = ip.split('.').count() == 4
        && ip
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if ok {
        Ok(())
    } else {
        Err(die(format!("invalid IPv4: {ip}")))
    }
}

pub fn is_confirmation_required(json: &str) -> bool {
    json.to_ascii_lowercase().contains("confirmation_required")
}

pub fn extract_confirmation_id(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    if let Some(id) = v
        .pointer("/confirmation/confirmation_id")
        .and_then(Value::as_str)
    {
        return Some(id.to_string());
    }
    find_confirmation_id(&v)
}

fn find_confirmation_id(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get("confirmation_id") {
                return Some(s.clone());
            }
            for child in map.values() {
                if let Some(s) = find_confirmation_id(child) {
                    return Some(s);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(find_confirmation_id),
        _ => None,
    }
}

pub fn ticket_request_body(
    department_id: &str,
    subject: &str,
    message: &str,
    priority: Option<TicketPriority>,
) -> Result<String> {
    if !department_id.bytes().all(|b| b.is_ascii_digit()) || department_id.is_empty() {
        return Err(die("--department-id must be digits"));
    }
    let dept: u64 = department_id
        .parse()
        .map_err(|_| die("--department-id must be digits"))?;
    let mut body = json!({
        "department_id": dept,
        "subject": subject,
        "message": message,
    });
    if let Some(p) = priority {
        body["priority"] = Value::String(p.as_str().to_string());
    }
    Ok(body.to_string())
}

pub fn rdns_set_body(ip: &str, hostname: &str) -> String {
    json!({ "ip": ip, "hostname": hostname }).to_string()
}

pub fn rdns_clear_body(ip: &str) -> String {
    json!({ "ip": ip }).to_string()
}

/// additionalProperties-safe walk: ignore unknown keys; never fail the whole
/// payload because the vendor added a field.
pub fn as_object(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    v.as_object()
}

pub fn data_value(root: &Value) -> &Value {
    root.get("data").unwrap_or(root)
}

pub fn json_string_field(obj: &Value, keys: &[&str]) -> String {
    let Some(map) = obj.as_object() else {
        return String::new();
    };
    for k in keys {
        if let Some(v) = map.get(*k) {
            match v {
                Value::Null => return String::new(),
                Value::String(s) => return s.clone(),
                Value::Number(n) => return n.to_string(),
                Value::Bool(b) => return b.to_string(),
                _ => {}
            }
        }
    }
    String::new()
}

pub fn display_opt(v: &Value) -> String {
    match v {
        Value::Null => "None".to_string(),
        Value::String(s) => format!("{s:?}").replace('"', "'"),
        other => other.to_string(),
    }
}
