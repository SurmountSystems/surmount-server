//! Never print Authorization, cookies, PEM, or tokens from journal lines.

/// Line-oriented redaction. PEM bodies after a BEGIN line are dropped until END.
#[derive(Debug, Default)]
pub struct RedactingStream {
    in_pem: bool,
}

impl RedactingStream {
    pub fn new() -> Self {
        Self { in_pem: false }
    }

    /// Return the line that may be printed, or `None` if this is swallowed PEM body.
    pub fn push_line(&mut self, line: &str) -> Option<String> {
        let is_begin = line.contains("-----BEGIN");
        let is_end = line.contains("-----END");
        if self.in_pem {
            if is_end {
                self.in_pem = false;
            }
            return None;
        }
        if is_begin {
            if !is_end {
                self.in_pem = true;
            }
            return Some("<redacted-pem>".to_string());
        }
        Some(redact_secret_line(line))
    }
}

/// Redact a single journal line (headers and values). PEM BEGIN is a full-line replace.
pub fn redact_secret_line(line: &str) -> String {
    if line.contains("-----BEGIN") {
        return "<redacted-pem>".to_string();
    }
    let lower = line.to_ascii_lowercase();
    let mut hit: Option<usize> = None;
    for needle in [
        "authorization:",
        "authorization=",
        "cookie:",
        "set-cookie:",
        "bearer ",
        concat!("token", "="),
        concat!("token", ":"),
        "admin_token",
        concat!("api_key", "="),
        "apikey=",
    ] {
        if let Some(idx) = lower.find(needle) {
            let end = idx + needle.len();
            hit = Some(match hit {
                Some(prev) if prev <= end => prev,
                _ => end,
            });
        }
    }
    match hit {
        Some(end) => {
            let end = end.min(line.len());
            format!("{}<redacted>", &line[..end])
        }
        None => line.to_string(),
    }
}

/// Redact a multi-line blob (status sections, tails).
pub fn redact_text(text: &str) -> String {
    let mut stream = RedactingStream::new();
    let mut out = String::new();
    let keep_trailing = text.ends_with('\n');
    let mut wrote = false;
    for line in text.lines() {
        if let Some(safe) = stream.push_line(line) {
            if wrote {
                out.push('\n');
            }
            out.push_str(&safe);
            wrote = true;
        }
    }
    if keep_trailing && wrote {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pem_begin_private() -> String {
        ["-----", "BEGIN", " ", "PRIVATE", " ", "KEY", "-----"].concat()
    }

    fn pem_end_private() -> String {
        ["-----", "END", " ", "PRIVATE", " ", "KEY", "-----"].concat()
    }

    #[test]
    fn redacts_authorization_header() {
        let line = "GET /  Authorization: Bearer SYNTHETIC-NOT-A-SECRET";
        let out = redact_secret_line(line);
        assert!(out.contains("Authorization:"), "{out}");
        assert!(out.contains("<redacted>"), "{out}");
        assert!(!out.contains("SYNTHETIC-NOT-A-SECRET"), "{out}");
        assert!(!out.contains("Bearer SYNTHETIC"), "{out}");
    }

    #[test]
    fn redacts_cookie_header() {
        let line = "Cookie: session=synthetic-session-value";
        let out = redact_secret_line(line);
        assert!(out.contains("Cookie:"), "{out}");
        assert!(!out.contains("synthetic-session-value"), "{out}");
    }

    #[test]
    fn redacts_token_assignment() {
        let line = "upstream token=SYNTHETICVALUE";
        let out = redact_secret_line(line);
        assert!(out.contains(concat!("token", "=")), "{out}");
        assert!(!out.contains("SYNTHETICVALUE"), "{out}");
    }

    #[test]
    fn redacts_pem_block_in_stream() {
        let mut s = RedactingStream::new();
        assert_eq!(
            s.push_line(&pem_begin_private()).as_deref(),
            Some("<redacted-pem>")
        );
        assert_eq!(s.push_line("SYNTHETIC-PEM-BODY-NOT-A-REAL-KEY"), None);
        assert_eq!(s.push_line(&pem_end_private()), None);
        assert_eq!(s.push_line("ok after pem").as_deref(), Some("ok after pem"));
    }

    #[test]
    fn redact_text_keeps_ordinary_journal_lines() {
        let text = "sshd[1]: Accepted publickey for nixbuilder\n";
        assert_eq!(redact_text(text), text);
    }
}
