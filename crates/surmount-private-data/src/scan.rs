//! Content and basename pattern classes.
//!
//! PEM headers are assembled at runtime so this file never contains a
//! contiguous matchable private-key header.

/// Whether path-gated classes (long SSH public keys, hosts IPv4) apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// `--paths`: always apply path-gated checks (fixtures live outside hosts/).
    Paths,
    /// `--staged` / `--tree`: SSH keys under hosts|secrets, IPv4 under hosts/.
    Product,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub class: &'static str,
    pub path: String,
}

pub fn scan_named(rel: &str, data: &[u8], gate: Gate) -> Vec<Hit> {
    let mut hits = Vec::new();
    hits.extend(check_basename(rel));
    if data.contains(&0) {
        return hits;
    }
    let text = String::from_utf8_lossy(data);
    hits.extend(scan_text(rel, text.as_ref(), gate));
    hits
}

fn check_basename(rel: &str) -> Vec<Hit> {
    let rel_norm = rel.trim_start_matches("./");
    if rel_norm == "secrets/README.md" {
        return Vec::new();
    }
    let base = rel_norm.rsplit('/').next().unwrap_or(rel_norm);
    let mut hits = Vec::new();
    let push = |hits: &mut Vec<Hit>, class: &'static str| {
        hits.push(Hit {
            class,
            path: rel.to_string(),
        });
    };

    if base == ".env" || base.starts_with(".env.") {
        push(&mut hits, "secret-basename:.env");
    }
    if base == "keys.txt" {
        push(&mut hits, "secret-basename:keys.txt");
    }
    if matches!(base, "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa") {
        push(&mut hits, "secret-basename:ssh-private-name");
    }
    if base == "secrets.yaml" || base == "secrets.yml" {
        push(&mut hits, "secret-basename:secrets.yaml");
    }
    if base.ends_with(".pem") {
        push(&mut hits, "secret-basename:pem");
    }
    if base.ends_with(".agekey") {
        push(&mut hits, "secret-basename:agekey");
    }
    if base.ends_with(".p12") || base.ends_with(".pfx") {
        push(&mut hits, "secret-basename:pkcs12");
    }
    if base.contains("decrypted") {
        push(&mut hits, "secret-basename:decrypted");
    }
    hits
}

fn scan_text(rel: &str, text: &str, gate: Gate) -> Vec<Hit> {
    let mut hits = Vec::new();
    let push = |hits: &mut Vec<Hit>, class: &'static str| {
        hits.push(Hit {
            class,
            path: rel.to_string(),
        });
    };

    if has_pem_private_key(text) {
        push(&mut hits, "pem-or-openssh-private-key");
    }
    if has_age_secret_key(text) {
        push(&mut hits, "age-secret-key");
    }
    if has_api_token_shape(text) {
        push(&mut hits, "api-token-shape");
    }
    if has_nostr_nsec(text) {
        push(&mut hits, "nostr-nsec");
    }
    if has_secret_assignment(text) {
        push(&mut hits, "secret-assignment");
    }

    if want_path_gated(rel, gate) && has_long_ssh_public_key(text) {
        push(&mut hits, "long-ssh-public-key");
    }
    if want_hosts_ipv4(rel, gate) && has_disallowed_ipv4(text) {
        push(&mut hits, "hosts-public-ipv4");
    }
    hits
}

fn want_path_gated(rel: &str, gate: Gate) -> bool {
    match gate {
        Gate::Paths => true,
        Gate::Product => path_is_hosts_or_secrets(rel),
    }
}

fn want_hosts_ipv4(rel: &str, gate: Gate) -> bool {
    match gate {
        Gate::Paths => true,
        Gate::Product => path_is_hosts(rel),
    }
}

fn path_is_hosts_or_secrets(rel: &str) -> bool {
    path_is_hosts(rel) || {
        let p = rel.trim_start_matches("./");
        p.starts_with("secrets/")
    }
}

fn path_is_hosts(rel: &str) -> bool {
    let p = rel.trim_start_matches("./");
    p.starts_with("hosts/")
}

fn pem_begin() -> String {
    format!("{}BEGIN", "-".repeat(5))
}

fn pem_end() -> String {
    "-".repeat(5)
}

fn has_pem_private_key(text: &str) -> bool {
    let begin = pem_begin();
    let end = pem_end();
    let openssh = format!("{begin} OPENSSH PRIVATE KEY{end}");
    let encrypted = format!("{begin} ENCRYPTED PRIVATE KEY{end}");
    if text.contains(&openssh) || text.contains(&encrypted) {
        return true;
    }
    // ripgrep: -----BEGIN ([A-Z0-9]+ )?PRIVATE KEY-----
    // always a space after BEGIN; optional LABEL then another space.
    let needle = format!("PRIVATE KEY{end}");
    let mut from = 0;
    while let Some(pos) = text[from..].find(&begin) {
        let abs = from + pos;
        let after_begin = &text[abs + begin.len()..];
        if let Some(stripped) = after_begin.strip_prefix(' ') {
            if stripped.starts_with(&needle) {
                return true;
            }
            let label_len = stripped
                .bytes()
                .take_while(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                .count();
            if label_len > 0 {
                if let Some(after_label) = stripped[label_len..].strip_prefix(' ') {
                    if after_label.starts_with(&needle) {
                        return true;
                    }
                }
            }
        }
        from = abs + 1;
    }
    false
}

fn has_age_secret_key(text: &str) -> bool {
    const PREFIX: &str = "AGE-SECRET-KEY-1";
    let mut from = 0;
    while let Some(pos) = text[from..].find(PREFIX) {
        let i = from + pos + PREFIX.len();
        let rest = text.as_bytes().get(i..).unwrap_or(&[]);
        if rest
            .iter()
            .take_while(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            .count()
            >= 1
        {
            return true;
        }
        from = from + pos + 1;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn bounded_at(bytes: &[u8], start: usize, end: usize) -> bool {
    let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
    let after_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
    before_ok && after_ok
}

fn find_prefix<'a>(bytes: &'a [u8], prefix: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    let mut from = 0;
    std::iter::from_fn(move || {
        if prefix.is_empty() || from + prefix.len() > bytes.len() {
            return None;
        }
        let window = &bytes[from..];
        if let Some(pos) = window.windows(prefix.len()).position(|w| w == prefix) {
            let abs = from + pos;
            from = abs + 1;
            Some(abs)
        } else {
            None
        }
    })
}

fn consume_while(bytes: &[u8], mut i: usize, pred: impl Fn(u8) -> bool) -> usize {
    while i < bytes.len() && pred(bytes[i]) {
        i += 1;
    }
    i
}

fn has_api_token_shape(text: &str) -> bool {
    let b = text.as_bytes();
    for start in find_prefix(b, b"ghp_") {
        let end = consume_while(b, start + 4, |c| c.is_ascii_alphanumeric());
        if end >= start + 4 + 20 && bounded_at(b, start, end) {
            return true;
        }
    }
    for start in find_prefix(b, b"github_pat_") {
        let end = consume_while(b, start + 11, |c| c.is_ascii_alphanumeric() || c == b'_');
        if end >= start + 11 + 20 && bounded_at(b, start, end) {
            return true;
        }
    }
    for start in find_prefix(b, b"sk_live_") {
        let end = consume_while(b, start + 8, |c| c.is_ascii_alphanumeric());
        if end >= start + 8 + 16 && bounded_at(b, start, end) {
            return true;
        }
    }
    for start in find_prefix(b, b"AKIA") {
        let end = consume_while(b, start + 4, |c| {
            c.is_ascii_uppercase() || c.is_ascii_digit()
        });
        if end == start + 4 + 16 && bounded_at(b, start, end) {
            return true;
        }
    }
    // xox[baprs]-[A-Za-z0-9-]{10,}
    for start in find_prefix(b, b"xox") {
        match b.get(start + 3) {
            Some(c) if b"baprs".contains(c) => {}
            _ => continue,
        }
        if b.get(start + 4) != Some(&b'-') {
            continue;
        }
        let end = consume_while(b, start + 5, |c| c.is_ascii_alphanumeric() || c == b'-');
        if end >= start + 5 + 10 && bounded_at(b, start, end) {
            return true;
        }
    }
    false
}

fn has_nostr_nsec(text: &str) -> bool {
    let b = text.as_bytes();
    for start in find_prefix(b, b"nsec1") {
        let end = consume_while(b, start + 5, |c| {
            c.is_ascii_lowercase() || c.is_ascii_digit()
        });
        if end >= start + 5 + 20 && bounded_at(b, start, end) {
            return true;
        }
    }
    false
}

fn has_secret_assignment(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    const KEYS: [&str; 4] = ["password", "secret", "token", "api_key"];
    for key in KEYS {
        let mut from = 0;
        let hay = lower.as_bytes();
        while let Some(pos) = hay[from..]
            .windows(key.len())
            .position(|w| w == key.as_bytes())
        {
            let start = from + pos;
            let end_key = start + key.len();
            if bounded_at(bytes, start, end_key) && quoted_assignment_value(&bytes[end_key..]) {
                return true;
            }
            from = start + 1;
        }
    }
    false
}

fn quoted_assignment_value(after_key: &[u8]) -> bool {
    let mut i = 0;
    while i < after_key.len() && after_key[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= after_key.len() || (after_key[i] != b'=' && after_key[i] != b':') {
        return false;
    }
    i += 1;
    while i < after_key.len() && after_key[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= after_key.len() || (after_key[i] != b'\'' && after_key[i] != b'"') {
        return false;
    }
    i += 1;
    let val_start = i;
    while i < after_key.len() && after_key[i] != b'\'' && after_key[i] != b'"' {
        i += 1;
    }
    if i >= after_key.len() {
        return false;
    }
    i - val_start >= 8
}

fn has_long_ssh_public_key(text: &str) -> bool {
    const ALGS: [&str; 4] = ["ssh-ed25519", "ssh-rsa", "ssh-ecdsa", "ssh-dss"];
    let b = text.as_bytes();
    for alg in ALGS {
        let ab = alg.as_bytes();
        for start in find_prefix(b, ab) {
            let mut i = start + ab.len();
            let mut saw_space = false;
            while i < b.len() && b[i].is_ascii_whitespace() {
                saw_space = true;
                i += 1;
            }
            if !saw_space {
                continue;
            }
            if i + 4 > b.len() || &b[i..i + 4] != b"AAAA" {
                continue;
            }
            let end = consume_while(b, i + 4, |c| {
                c.is_ascii_alphanumeric() || c == b'+' || c == b'/'
            });
            if end >= i + 4 + 40 {
                return true;
            }
        }
    }
    false
}

fn has_disallowed_ipv4(text: &str) -> bool {
    for ip in ipv4_candidates(text) {
        if !ipv4_is_allowed(&ip) {
            return true;
        }
    }
    false
}

fn ipv4_candidates(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if let Some((end, ip)) = match_ipv4_at(b, i) {
            out.push(ip);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn match_ipv4_at(b: &[u8], start: usize) -> Option<(usize, String)> {
    if start > 0 && b[start - 1].is_ascii_digit() {
        return None;
    }
    if !b.get(start).is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut i = start;
    for part in 0..4 {
        let mut digits = 0;
        while i < b.len() && b[i].is_ascii_digit() && digits < 3 {
            i += 1;
            digits += 1;
        }
        if digits == 0 {
            return None;
        }
        if part < 3 {
            if i >= b.len() || b[i] != b'.' {
                return None;
            }
            i += 1;
        }
    }
    Some((i, String::from_utf8_lossy(&b[start..i]).into_owned()))
}

fn ipv4_is_allowed(ip: &str) -> bool {
    if ip == "0.0.0.0" {
        return true;
    }
    if ip.starts_with("127.")
        || ip.starts_with("10.")
        || ip.starts_with("192.168.")
        || ip.starts_with("169.254.")
        || ip.starts_with("203.0.113.")
        || ip.starts_with("198.51.100.")
        || ip.starts_with("192.0.2.")
    {
        return true;
    }
    // 172.16.0.0/12
    if let Some(rest) = ip.strip_prefix("172.") {
        let second = rest.split('.').next().unwrap_or("");
        if let Ok(n) = second.parse::<u32>() {
            if (16..=31).contains(&n) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assembled_rsa_pem() -> String {
        let begin = pem_begin();
        let end = pem_end();
        format!("{begin} RSA PRIVATE KEY{end}\nMIIBfake\n")
    }

    fn assembled_pkcs8_pem() -> String {
        let begin = pem_begin();
        let end = pem_end();
        format!("{begin} PRIVATE KEY{end}\nMIIBfake\n")
    }

    #[test]
    fn rsa_and_pkcs8_private_headers_are_hits() {
        let rsa = scan_named("k.txt", assembled_rsa_pem().as_bytes(), Gate::Product);
        assert!(
            rsa.iter().any(|h| h.class == "pem-or-openssh-private-key"),
            "{rsa:?}"
        );
        let pkcs8 = scan_named("k.txt", assembled_pkcs8_pem().as_bytes(), Gate::Product);
        assert!(
            pkcs8
                .iter()
                .any(|h| h.class == "pem-or-openssh-private-key"),
            "{pkcs8:?}"
        );
    }

    #[test]
    fn rfc1918_172_15_is_not_in_allowlist() {
        let hits = scan_named("hosts/x.nix", b"address = \"172.15.0.1\";\n", Gate::Product);
        assert!(
            hits.iter().any(|h| h.class == "hosts-public-ipv4"),
            "{hits:?}"
        );
    }
}
