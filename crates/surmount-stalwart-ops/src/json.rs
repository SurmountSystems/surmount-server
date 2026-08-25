use serde_json::Value;

/// Domain id whose `name` matches, never the first id in the blob.
pub fn find_domain_id(out: &str, name: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<Value>(out) {
        if let Some(id) = walk_named_id(&v, name) {
            return Some(id);
        }
    }
    // Compact array of objects.
    if let Some(items) = extract_items(out) {
        for item in items {
            if item.get("name").and_then(Value::as_str) == Some(name) {
                if let Some(id) = item.get("id").and_then(Value::as_str) {
                    return Some(id.to_string());
                }
            }
        }
    }
    // Table: first column id when a later field equals name.
    for line in out.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() > 1 && cols.iter().skip(1).any(|c| *c == name) && cols[0] != name {
            return Some(cols[0].trim_end_matches('\r').to_string());
        }
    }
    None
}

fn walk_named_id(v: &Value, name: &str) -> Option<String> {
    match v {
        Value::Object(m) => {
            if m.get("name").and_then(Value::as_str) == Some(name) {
                if let Some(id) = m.get("id").and_then(Value::as_str) {
                    return Some(id.to_string());
                }
            }
            for val in m.values() {
                if let Some(id) = walk_named_id(val, name) {
                    return Some(id);
                }
            }
            None
        }
        Value::Array(a) => a.iter().find_map(|x| walk_named_id(x, name)),
        _ => None,
    }
}

fn extract_items(out: &str) -> Option<Vec<Value>> {
    if let Ok(v) = serde_json::from_str::<Value>(out) {
        match v {
            Value::Array(a) => return Some(a),
            Value::Object(m) => {
                if let Some(Value::Array(a)) = m.get("items") {
                    return Some(a.clone());
                }
            }
            _ => {}
        }
    }
    None
}

pub fn query_has_selector(out: &str, selector: &str, domain_id: &str) -> bool {
    if let Some(items) = extract_items(out) {
        for item in items {
            let sel = item.get("selector").and_then(Value::as_str);
            let did = item.get("domainId").and_then(Value::as_str);
            if sel == Some(selector) && did == Some(domain_id) {
                return true;
            }
        }
    }
    // Flattened JSON objects.
    out.contains(&format!("\"selector\":\"{selector}\""))
        && out.contains(&format!("\"domainId\":\"{domain_id}\""))
}

pub fn selector_stage_active(out: &str, selector: &str, domain_id: &str) -> bool {
    if let Some(items) = extract_items(out) {
        for item in items {
            let sel = item.get("selector").and_then(Value::as_str);
            let did = item.get("domainId").and_then(Value::as_str);
            let st = item.get("stage").and_then(Value::as_str);
            if sel == Some(selector) && did == Some(domain_id) && st == Some("active") {
                return true;
            }
        }
    }
    false
}

pub fn extract_secret_line(out: &str) -> Option<String> {
    for line in out.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix(concat!("Secret", ":")) {
            let s = rest.trim();
            if !s.is_empty() && !s.contains('\n') {
                return Some(s.to_string());
            }
        }
    }
    None
}

pub fn extract_created_id(out: &str, kind: &str) -> Option<String> {
    let lower_kind = kind.to_ascii_lowercase();
    for line in out.lines() {
        let t = line.trim_start();
        let tl = t.to_ascii_lowercase();
        if let Some(rest) = tl.strip_prefix("created ") {
            if rest.starts_with(&lower_kind) {
                let orig = t.split_whitespace().nth(2)?;
                return Some(orig.trim_end_matches('\r').to_string());
            }
        }
    }
    None
}

pub fn account_query_has_admin(out: &str, localpart: &str, domain: &str) -> bool {
    let email = format!("{localpart}@{domain}");
    let has_identity = out.contains(&format!("\"name\":\"{localpart}\""))
        || out.contains(&format!("\"name\": \"{localpart}\""))
        || out.contains(&format!("\"emailAddress\":\"{email}\""))
        || out.contains(&format!("\"emailAddress\": \"{email}\""))
        || out.contains(&email);
    let has_admin = out.contains("\"@type\":\"Admin\"")
        || out.contains("\"@type\": \"Admin\"")
        || out.contains("\"roles\":{\"@type\":\"Admin\"}")
        || (out.contains("\"roles\"") && out.contains("Admin"));
    has_identity && has_admin
}

pub fn hostname_is_expected(name: &str, expected: &str) -> bool {
    expected.split(',').any(|p| p.trim() == name)
}

/// Resolve File / Let's Encrypt Certificate id (not first-boot rcgen).
pub fn file_cert_id_in_query(out: &str, path: &str, expected: &str) -> Option<String> {
    if let Some(items) = extract_items(out) {
        let mut best: Option<(String, usize)> = None;
        for item in items {
            let id = item.get("id").and_then(Value::as_str)?;
            if let Some(fp) = item.get("filePath").and_then(Value::as_str) {
                if fp == path {
                    return Some(id.to_string());
                }
            }
            if let Some(cert) = item.get("certificate") {
                if cert.get("filePath").and_then(Value::as_str) == Some(path) {
                    return Some(id.to_string());
                }
            }
            let mut hits = 0usize;
            collect_host_hits(item, expected, &mut hits);
            if hits >= 2 && best.as_ref().map(|b| hits > b.1).unwrap_or(true) {
                best = Some((id.to_string(), hits));
            } else if hits >= 1 && best.as_ref().map(|b| b.1 == 0).unwrap_or(true) {
                let low = id.to_ascii_lowercase();
                if !low.contains("rcgen") && !low.contains("self") && !low.contains("firstboot") {
                    best = Some((id.to_string(), hits));
                }
            }
        }
        return best.map(|b| b.0);
    }
    // Line-oriented fallback (0.16.15 flattened JSON).
    let mut last_id = String::new();
    let mut hits = 0usize;
    let mut saw_path = false;
    let mut best_id = String::new();
    let mut best_hits = 0usize;
    let flat = out.replace(['{', '}', ',', '[', ']'], "\n");
    for line in flat.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(id) = json_string_after(line, "id") {
            if saw_path && !last_id.is_empty() {
                return Some(last_id);
            }
            if hits >= 2 && hits > best_hits {
                best_id = last_id.clone();
                best_hits = hits;
            } else if hits >= 1 && best_hits == 0 {
                let low = last_id.to_ascii_lowercase();
                if !low.contains("rcgen") && !low.contains("self") && !low.contains("firstboot") {
                    best_id = last_id.clone();
                    best_hits = hits;
                }
            }
            last_id = id;
            hits = 0;
            saw_path = false;
            continue;
        }
        if let Some(fp) = json_string_after(line, "filePath") {
            if fp == path && !last_id.is_empty() {
                saw_path = true;
            }
            continue;
        }
        if line.starts_with('"') && line.ends_with('"') && line.len() > 2 {
            let token = &line[1..line.len() - 1];
            if hostname_is_expected(token, expected) {
                hits += 1;
            }
        }
    }
    if saw_path && !last_id.is_empty() {
        return Some(last_id);
    }
    if hits >= 2 && hits > best_hits {
        best_id = last_id;
        best_hits = hits;
    }
    if best_hits > 0 {
        Some(best_id)
    } else {
        None
    }
}

fn collect_host_hits(v: &Value, expected: &str, hits: &mut usize) {
    match v {
        Value::String(s) => {
            if hostname_is_expected(s, expected) {
                *hits += 1;
            }
        }
        Value::Array(a) => {
            for x in a {
                collect_host_hits(x, expected, hits);
            }
        }
        Value::Object(m) => {
            for x in m.values() {
                collect_host_hits(x, expected, hits);
            }
        }
        _ => {}
    }
}

fn json_string_after(line: &str, field: &str) -> Option<String> {
    let pat = format!("\"{field}\"");
    let i = line.find(&pat)?;
    let rest = &line[i + pat.len()..];
    let colon = rest.find(':')?;
    let rest = rest[colon + 1..].trim();
    if let Some(s) = rest.strip_prefix('"') {
        let end = s.find('"')?;
        return Some(s[..end].to_string());
    }
    None
}

pub fn query_has_file_path(out: &str, path: &str) -> bool {
    out.contains(&format!("\"filePath\":\"{path}\""))
        || out.contains(&format!("\"filePath\": \"{path}\""))
}
