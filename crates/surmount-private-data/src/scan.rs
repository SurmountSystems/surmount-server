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
    if has_secret_assignment(rel, text) {
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

// secret-assignment: a secret-named key followed by a literal value.
//
// Operator direction 2026-11-10: catch real secret values whether quoted or
// not, and err on the side of protection. Choices, so a reviewer can audit
// them without reading the code twice:
//
// 1. Key names. The identifier just before the separator is split on `_`,
//    `-`, `.`, and camelCase. Its last word (or last two words) must be a
//    secret noun: password, passwd, pwd, pass, passphrase, secret, token,
//    credential(s), apikey / api key, access key, private key / privkey,
//    secret key, signing key, encryption key, client secret. Run-together
//    names that end in password, passwd, secret, token, or apikey count too
//    (PGPASSWORD, csrftoken). A trailing value, base, hash,
//    b64, base64, hex, raw, plain, or plaintext word is looked through, so
//    `SECRET_KEY_BASE` and `password_hash` still count. Embedded keys such
//    as `DB_PASSWORD`, `GITHUB_TOKEN`, `--api-key`, and `clientSecret`
//    count. A different last word means the value is not the secret
//    itself: `passwordFile`, `token_path`, `secret_dir`, `TOKEN_URL`.
// 2. Separators: `=`, `:`, `:=`, `+=`, after optional spaces and an
//    optional closing quote on the key (JSON and YAML `"password": ...`).
//    Comparisons (`==`, `!=`, `<=`, `>=`, `=~`, `=>`) and Rust paths (`::`)
//    are not separators. A `--flag value` pair counts in non-code files.
//    `Authorization: Bearer x` and any `Bearer <token>` count.
// 3. Values. A quoted value ('...', "...", `...`, also Rust b"..." and
//    r"...") is read to its closing quote on the same line. An unquoted
//    value is read to whitespace or one of ; , & ) ] } > | and quotes.
//    That covers `export API_TOKEN=x`, `--password=x`, URL `?token=x`,
//    `.env`, YAML, TOML, and INI. A value of 4 or more bytes counts. The
//    old rule needed a quoted value of 8 or more.
// 4. Not a secret value (skipped):
//    a. empty, or shorter than 4 bytes;
//    b. a variable or template reference: starts with $ (but a crypt hash
//       such as `$6$...` or `$argon2id$...` still counts), %, {, <, @, !,
//       *, #, or `...`;
//    c. a path or URL: starts with /, ./, ../, ~/, or contains `://`;
//    d. a placeholder: contains example, fake, dummy, sample,
//       placeholder, synth, notreal, not-real, not_real, not-a-real,
//       changeme, change-me, change_me, redacted, replace, xxxx, test,
//       your-, your_, `<`, or `...`; or is a keyword such as true, false,
//       null, none, required, string, missing, same-origin;
//    e. a lowercase slash-separated name such as `mail/accounts/admin`
//       (a sops secret name or relative path; base64 mixes case and still
//       counts);
//    f. separated words where one word is itself a secret noun, such as
//       `secret-value` or `super-secret-token` (written-out stand-ins);
//    g. an upper-case name with no digits, quoted or not, such as
//       `STALWART_TOKEN`, `"STALWART_RECOVERY_ADMIN"`, or a usage
//       metavariable `FILE`. Nix and JSON quote those names. A value
//       with a digit or a lowercase letter still counts;
//    h. an unquoted value that looks like code: contains `(` or `::`, or
//       ends in `?`;
//    i. in Rust and Nix (and other code) files, an unquoted value after a
//       spaced separator or after `:`. There the right-hand side is an
//       expression, variable, or type (`password: String`,
//       `token = mkOption`), not a literal. Code still counts quoted
//       literals, and still counts unspaced `KEY=value` (shell or URL text
//       inside a string), because rustfmt and nixfmt always put spaces
//       around real code `=`;
//    j. an unquoted all-letters word followed by more words on the same
//       line in prose (`the token: generated by ...`).
// 5. Test values that contain a placeholder word (for example
//    `unit-test-only-secret`) are skipped on purpose. A real secret that
//    happens to contain one of those words is missed. That is the known
//    cost of keeping the gate usable on this tree.

const SECRET_NOUNS: [&str; 11] = [
    "password",
    "passwd",
    "pwd",
    "pass",
    "passphrase",
    "secret",
    "token",
    "credential",
    "credentials",
    "apikey",
    "privkey",
];

/// Two-word keys, last word first after look-through.
const SECRET_PAIRS: [(&str, &str); 6] = [
    ("api", "key"),
    ("access", "key"),
    ("private", "key"),
    ("secret", "key"),
    ("signing", "key"),
    ("encryption", "key"),
];

const LOOK_THROUGH: [&str; 9] = [
    "value",
    "base",
    "hash",
    "b64",
    "base64",
    "hex",
    "raw",
    "plain",
    "plaintext",
];

const PLACEHOLDER_PARTS: [&str; 22] = [
    "...",
    "synth",
    "not-a-real",
    "example",
    "fake",
    "dummy",
    "sample",
    "placeholder",
    "synthetic",
    "notreal",
    "not-real",
    "not_real",
    "changeme",
    "change-me",
    "change_me",
    "redacted",
    "xxxx",
    "test",
    "your-",
    "your_",
    "replace",
    "<",
];

const KEYWORD_VALUES: [&str; 27] = [
    "missing",
    "present",
    "same-origin",
    "include",
    "omit",
    "true",
    "false",
    "null",
    "none",
    "nil",
    "undefined",
    "required",
    "optional",
    "string",
    "boolean",
    "integer",
    "number",
    "object",
    "array",
    "default",
    "hidden",
    "masked",
    "unset",
    "empty",
    "secret",
    "password",
    "token",
];

const MIN_VALUE_LEN: usize = 4;

fn is_key_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'
}

/// Split an identifier into lowercase words on `_ - .` and camelCase.
fn ident_words(ident: &[u8]) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for &b in ident {
        if b == b'_' || b == b'-' || b == b'.' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if b.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        prev_lower = b.is_ascii_lowercase() || b.is_ascii_digit();
        cur.push(b.to_ascii_lowercase() as char);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn is_secret_key(ident: &[u8]) -> bool {
    let mut words = ident_words(ident);
    while words.len() > 1
        && words
            .last()
            .is_some_and(|w| LOOK_THROUGH.contains(&w.as_str()))
    {
        words.pop();
    }
    let Some(last) = words.last() else {
        return false;
    };
    if SECRET_NOUNS.contains(&last.as_str()) {
        return true;
    }
    // Run-together names such as PGPASSWORD, csrftoken, or adminsecret.
    // Only the long nouns: a `pass` or `pwd` suffix (bypass, compass) is
    // too common in ordinary words.
    if ["password", "passwd", "secret", "token", "apikey"]
        .iter()
        .any(|n| last.len() > n.len() && last.ends_with(n))
    {
        return true;
    }
    if words.len() >= 2 {
        let prev = words[words.len() - 2].as_str();
        if SECRET_PAIRS
            .iter()
            .any(|(a, b)| *a == prev && *b == last.as_str())
        {
            return true;
        }
    }
    false
}

/// Rust, Nix, and similar source files, where an unquoted right-hand side
/// is an expression rather than a literal.
fn is_code_path(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    let Some((_, ext)) = base.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "rs" | "nix"
            | "go"
            | "js"
            | "mjs"
            | "cjs"
            | "ts"
            | "tsx"
            | "jsx"
            | "py"
            | "c"
            | "h"
            | "cc"
            | "cpp"
            | "hpp"
            | "java"
            | "kt"
            | "swift"
            | "rb"
            | "lua"
            | "php"
            | "cs"
            | "scala"
            | "zig"
    )
}

fn is_crypt_hash(v: &[u8]) -> bool {
    // `$6$salt$hash`, `$2b$12$...`, `$argon2id$v=19$...`, `$y$...`
    if v.first() != Some(&b'$') {
        return false;
    }
    let id_len = v[1..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    id_len > 0 && v.get(1 + id_len) == Some(&b'$') && v.len() >= 1 + id_len + 8
}

fn value_is_secret_literal(v: &[u8]) -> bool {
    if v.len() < MIN_VALUE_LEN {
        return false;
    }
    if is_crypt_hash(v) {
        return true;
    }
    // Quoted env names (`secretKey = "STALWART_RECOVERY_ADMIN"`) are the
    // same class as the unquoted form. Digits and lowercase still count.
    if is_upper_name(v) {
        return false;
    }
    if matches!(
        v[0],
        b'$' | b'%' | b'{' | b'<' | b'@' | b'!' | b'*' | b'#' | b'/' | b'~'
    ) {
        return false;
    }
    if v.starts_with(b"./") || v.starts_with(b"../") || v.starts_with(b"...") {
        return false;
    }
    let lower = String::from_utf8_lossy(v).to_ascii_lowercase();
    if lower.contains("://") {
        return false;
    }
    if PLACEHOLDER_PARTS.iter().any(|p| lower.contains(p)) {
        return false;
    }
    if KEYWORD_VALUES.contains(&lower.as_str()) {
        return false;
    }
    if is_relative_path_name(v) || is_noun_phrase_placeholder(&lower) {
        return false;
    }
    true
}

/// `mail/accounts/admin`: a lowercase slash-separated name, such as a sops
/// secret name or a relative path. Base64 secrets mix case, so they still
/// count.
fn is_relative_path_name(v: &[u8]) -> bool {
    v.contains(&b'/')
        && v.iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'/' | b'_' | b'-' | b'.')
        })
}

/// `secret-value`, `super-secret-token`, `my_password_here`: a value made
/// of separated words, one of which is itself a secret noun. Those are
/// written-out stand-ins, not generated secrets. A run-together value such
/// as `MySecretPassword1` has no separator and still counts.
fn is_noun_phrase_placeholder(lower: &str) -> bool {
    if !lower.contains(['-', '_']) {
        return false;
    }
    lower.split(['-', '_']).any(|w| {
        matches!(
            w,
            "secret" | "token" | "password" | "passwd" | "key" | "apikey"
        )
    })
}

/// `STALWART_TOKEN`, `"STALWART_RECOVERY_ADMIN"`, `FILE`, `PATH`: an
/// upper-case name with no digits, quoted or not. Read as an environment
/// variable or a usage metavariable. A digit or a lowercase letter still
/// counts as a literal.
fn is_upper_name(v: &[u8]) -> bool {
    v.first().is_some_and(|c| c.is_ascii_uppercase())
        && v.iter().all(|c| c.is_ascii_uppercase() || *c == b'_')
}

fn skip_inline_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

/// Read the value that starts at `i`. Returns (value, was_quoted).
fn read_value(b: &[u8], i: usize) -> Option<(&[u8], bool)> {
    let mut i = i;
    if i >= b.len() {
        return None;
    }
    // Rust byte and raw string prefixes: b"..", r"..", br"..", r#".."#.
    let mut j = i;
    while j < b.len() && j < i + 2 && (b[j] == b'b' || b[j] == b'r') {
        j += 1;
    }
    while j < b.len() && b[j] == b'#' && j > i {
        j += 1;
    }
    if j > i && j < b.len() && b[j] == b'"' {
        i = j;
    }
    let q = b[i];
    if q == b'"' || q == b'\'' || q == b'`' {
        let start = i + 1;
        let mut k = start;
        while k < b.len() && b[k] != q && b[k] != b'\n' {
            if b[k] == b'\\' {
                k += 1;
            }
            k += 1;
        }
        if k >= b.len() || b[k] != q {
            return None;
        }
        return Some((&b[start..k], true));
    }
    let start = i;
    let mut k = start;
    while k < b.len()
        && !b[k].is_ascii_whitespace()
        && !matches!(
            b[k],
            b';' | b',' | b'&' | b')' | b']' | b'}' | b'>' | b'|' | b'"' | b'\'' | b'`'
        )
    {
        k += 1;
    }
    Some((&b[start..k], false))
}

fn unquoted_is_code_like(v: &[u8]) -> bool {
    v.contains(&b'(') || v.windows(2).any(|w| w == b"::") || v.ends_with(b"?")
}

/// Prose such as `the token: generated by ...`: an all-letters word with
/// more words after it on the same line.
fn unquoted_is_prose(b: &[u8], v: &[u8], end: usize) -> bool {
    if !v.iter().all(|c| c.is_ascii_alphabetic() || *c == b'-') {
        return false;
    }
    let k = skip_inline_ws(b, end);
    k > end && k < b.len() && b[k].is_ascii_alphabetic()
}

fn assignment_hit(b: &[u8], sep: usize, sep_len: usize, code: bool) -> bool {
    // Walk back over spaces and one optional closing quote to the key.
    let mut k = sep;
    while k > 0 && (b[k - 1] == b' ' || b[k - 1] == b'\t') {
        k -= 1;
    }
    let spaced_before = k != sep;
    let mut key_quote = None;
    if k > 0 && (b[k - 1] == b'"' || b[k - 1] == b'\'') {
        key_quote = Some(b[k - 1]);
        k -= 1;
    }
    let key_end = k;
    while k > 0 && is_key_byte(b[k - 1]) {
        k -= 1;
    }
    let key_start = k;
    if key_start == key_end || !is_secret_key(&b[key_start..key_end]) {
        return false;
    }
    // `"token="` inside a quoted string followed by its closing quote: the
    // quote after the separator closes the string; there is no value.
    let opener_quote = key_start > 0 && matches!(b[key_start - 1], b'"' | b'\'' | b'`');
    let after = sep + sep_len;
    if key_quote.is_none() && opener_quote && b.get(after) == Some(&b[key_start - 1]) {
        return false;
    }
    let vstart = skip_inline_ws(b, after);
    let spaced_after = vstart != after;
    let Some((v, quoted)) = read_value(b, vstart) else {
        return false;
    };
    if !quoted {
        if code && (spaced_before || spaced_after || b[sep] == b':') {
            return false;
        }
        if unquoted_is_code_like(v) || is_upper_name(v) {
            return false;
        }
        let vend = vstart + v.len();
        if unquoted_is_prose(b, v, vend) {
            return false;
        }
        // `Authorization: Bearer <token>`: judge the token, not the scheme.
        let lower = v.to_ascii_lowercase();
        if matches!(lower.as_slice(), b"bearer" | b"basic" | b"token") {
            let t = skip_inline_ws(b, vend);
            if let Some((tv, _)) = read_value(b, t) {
                return value_is_secret_literal(tv);
            }
            return false;
        }
    }
    value_is_secret_literal(v)
}

/// `--password hunter22` style flag pairs (non-code files only).
fn flag_pair_hit(b: &[u8], dash: usize) -> bool {
    let mut k = dash;
    while k < b.len() && b[k] == b'-' {
        k += 1;
    }
    let key_start = k;
    while k < b.len() && is_key_byte(b[k]) && b[k] != b'.' {
        k += 1;
    }
    if k == key_start || !is_secret_key(&b[key_start..k]) {
        return false;
    }
    let v = skip_inline_ws(b, k);
    if v == k || v >= b.len() || b[v] == b'-' || b[v] == b'\\' {
        return false;
    }
    match read_value(b, v) {
        Some((val, quoted)) => {
            (quoted || (!unquoted_is_prose(b, val, v + val.len()) && !is_upper_name(val)))
                && value_is_secret_literal(val)
        }
        None => false,
    }
}

/// `Bearer <token>` anywhere: a long token-shaped value after the scheme.
fn bearer_hit(b: &[u8], start: usize) -> bool {
    let end = start + 6;
    if start > 0 && is_word_byte(b[start - 1]) {
        return false;
    }
    let v = skip_inline_ws(b, end);
    if v == end {
        return false;
    }
    let tok_end = consume_while(b, v, |c| {
        c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'~' | b'+' | b'/' | b'=' | b'-')
    });
    tok_end >= v + 16 && value_is_secret_literal(&b[v..tok_end])
}

fn has_secret_assignment(rel: &str, text: &str) -> bool {
    let b = text.as_bytes();
    let code = is_code_path(rel);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let prev = if i > 0 { b[i - 1] } else { 0 };
        let next = b.get(i + 1).copied().unwrap_or(0);
        let hit = match c {
            b'=' => {
                let comparison =
                    matches!(prev, b'=' | b'!' | b'<' | b'>') || matches!(next, b'=' | b'~' | b'>');
                // `:=` and `+=` are handled at the `=` with the key before
                // the `:` / `+`.
                if comparison {
                    false
                } else if prev == b':' || prev == b'+' {
                    assignment_hit(b, i - 1, 2, code)
                } else {
                    assignment_hit(b, i, 1, code)
                }
            }
            b':' => {
                if prev == b':' || next == b':' || next == b'=' {
                    false
                } else {
                    assignment_hit(b, i, 1, code)
                }
            }
            b'-' if !code && next == b'-' && prev != b'-' => flag_pair_hit(b, i),
            b'b' | b'B' if b.len() >= i + 6 && b[i..i + 6].eq_ignore_ascii_case(b"bearer") => {
                bearer_hit(b, i)
            }
            _ => false,
        };
        if hit {
            return true;
        }
        i += 1;
    }
    false
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

    fn assigns(rel: &str, body: &str) -> bool {
        scan_named(rel, body.as_bytes(), Gate::Product)
            .iter()
            .any(|h| h.class == "secret-assignment")
    }

    #[test]
    fn secret_assignment_catches_unquoted_and_short_values() {
        // Synthetic values only. None of these is a real credential.
        let cases = [
            ("notes.txt", "token=abc12345\n"),
            ("config.yaml", "password: hunter2\n"),
            ("config.yaml", "db:\n  password: 'q9Lm'\n"),
            ("run.sh", "export API_TOKEN=Zq81mXk0Pw\n"),
            ("run.sh", "API_TOKEN=Zq81mXk0Pw ./serve\n"),
            (
                "run.sh",
                "curl --password=Zq81mXk0Pw https://host.invalid/\n",
            ),
            ("run.sh", "tool --api-key Zq81mXk0Pw list\n"),
            (
                "notes.md",
                "open https://host.invalid/a?user=x&token=Zq81mXk0Pw&b=1\n",
            ),
            ("data.json", "{\"password\": \"Zq81mXk0Pw\"}\n"),
            ("data.json", "{\"clientSecret\":\"Zq81mXk0Pw\"}\n"),
            (".env.sample", "DB_PASSWORD=Zq81mXk0Pw\n"),
            ("ci.env", "GITHUB_TOKEN=Zq81mXk0Pw\n"),
            ("run.sh", "PGPASSWORD=Zq81mXk0Pw psql\n"),
            ("app.toml", "secret_key = \"Zq81mXk0Pw\"\n"),
            ("app.ini", "aws_secret_access_key = Zq81mXk0Pw\n"),
            ("app.ini", "SECRET_KEY_BASE=Zq81mXk0Pw\n"),
            ("app.ini", "passwd := Zq81mXk0Pw\n"),
            ("app.ini", "private_key = Zq81mXk0Pw\n"),
            ("app.ini", "credentials = Zq81mXk0Pw\n"),
            ("req.http", "Authorization: Bearer Zq81mXk0PwZq81mXk0Pw\n"),
            ("req.http", "curl -H 'X: Bearer Zq81mXk0PwZq81mXk0Pw'\n"),
            ("main.rs", "let password = \"Zq81mXk0Pw\";\n"),
            (
                "main.rs",
                "let k = b\"Zq81mXk0Pw\"; let api_key = r\"Zq81mXk0Pw\";\n",
            ),
            ("main.rs", "let cmd = \"env API_TOKEN=Zq81mXk0Pw serve\";\n"),
            ("host.nix", "{ smtpPassword = \"Zq81mXk0Pw\"; }\n"),
            // Same key shape as an env-var binding. Mixed case and digits
            // are still literals. An all-caps name with no digits is not.
            ("host.nix", "{ secretKey = \"Zq81mXk0Pw\"; }\n"),
            ("host.nix", "{ secretKey = \"ABCD1234\"; }\n"),
            (
                "host.nix",
                "{ hashedPassword = \"$6$saltsalt$Zq81mXk0PwZq81mXk0Pw\"; }\n",
            ),
        ];
        for (rel, body) in cases {
            assert!(assigns(rel, body), "should hit: {rel}");
        }
    }

    #[test]
    fn secret_assignment_skips_references_placeholders_and_code() {
        let cases = [
            // Empty and too short.
            ("notes.txt", "token=\n"),
            ("config.yaml", "password:\n  file: x\n"),
            ("config.yaml", "password: abc\n"),
            // Variable, command, and template references.
            ("run.sh", "TOKEN=$(cat \"$TOKEN_FILE\")\n"),
            ("run.sh", "local password=\"$1\"\n"),
            ("run.sh", "export API_TOKEN=${API_TOKEN:-}\n"),
            ("run.bat", "set PASSWORD=%PASSWORD%\n"),
            ("tpl.yaml", "password: \"{{ vault_password }}\"\n"),
            ("tpl.yaml", "password: <your password>\n"),
            ("tpl.yaml", "token: !vault |\n"),
            ("tpl.yaml", "token: *shared\n"),
            ("pkg.nix", "{ password = \"@password@\"; }\n"),
            ("main.rs", "let u = format!(\"/a?token={tok}\");\n"),
            // Paths, URLs, and secret names.
            ("host.nix", "{ passwordFile = \"/run/secrets/x\"; }\n"),
            ("host.nix", "{ token = \"/run/secrets/x\"; }\n"),
            (
                "host.nix",
                "{ passwordSecret = \"mail/accounts/admin\"; }\n",
            ),
            ("app.ini", "token_url = https://host.invalid/token\n"),
            ("app.ini", "secret = ./secret.txt\n"),
            // Placeholders and test stand-ins.
            ("app.ini", "password = changeme\n"),
            ("app.ini", "password = \"example-password\"\n"),
            ("main.rs", "let secret = \"unit-test-only-secret\";\n"),
            ("main.rs", "let h = \"Bearer super-secret-token\";\n"),
            ("doc.md", "ApiKey=shc_live_...\n"),
            ("doc.md", "`just x -- --credentials FILE -- list`\n"),
            ("doc.md", "# Token: STALWART_TOKEN or the token file\n"),
            ("doc.md", "the token: generated by the host\n"),
            ("doc.md", "no `TOKEN=` prefix and a `Secret:` line\n"),
            // Keyword values.
            ("app.yaml", "password: false\n"),
            ("pages.rs", "fetch(u, { credentials: 'same-origin' })\n"),
            // Code: an unquoted right-hand side is an expression or type.
            ("main.rs", "struct A { password: String, token: &str }\n"),
            ("main.rs", "let token = load_token(&token_file)?;\n"),
            ("main.rs", "let auth = auth::AuthConfig::off();\n"),
            ("host.nix", "{ secretKey = \"STALWART_RECOVERY_ADMIN\"; }\n"),
            (
                "host.nix",
                "{ password = mkOption { type = types.str; }; }\n",
            ),
            ("host.nix", "{ secret = config.sops.secrets.x.path; }\n"),
            // Comparisons are not assignments.
            (
                "run.sh",
                "if [[ \"$part\" =~ (token|password|secret)= ]]; then\n",
            ),
            (
                "run.sh",
                "if [[ \"$part\" == *\"token=\"* || \"$part\" == *\"password=\"* ]]; then\n",
            ),
            ("main.rs", "if token == \"Zq81mXk0Pw\" {}\n"),
        ];
        for (rel, body) in cases {
            assert!(!assigns(rel, body), "should not hit: {rel}: {body}");
        }
    }

    #[test]
    fn secret_key_names_split_identifiers() {
        for k in [
            "password",
            "DB_PASSWORD",
            "GITHUB_TOKEN",
            "--api-key",
            "apiKey",
            "clientSecret",
            "aws_secret_access_key",
            "SECRET_KEY_BASE",
            "password_hash",
            "spring.datasource.password",
            "pwd",
            "privkey",
            "PGPASSWORD",
            "csrftoken",
        ] {
            assert!(is_secret_key(k.as_bytes()), "{k}");
        }
        for k in [
            "passwordFile",
            "token_path",
            "secret_dir",
            "TOKEN_URL",
            "bypass",
            "keyboard",
            "tokenizer",
            "auth_mode",
            "compass",
        ] {
            assert!(!is_secret_key(k.as_bytes()), "{k}");
        }
    }
}
