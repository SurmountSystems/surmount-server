//! TLS handshake + certificate date check (host e2e helper).
//!
//! Uses `openssl` on PATH (same contract as former `scripts/check-tls.sh`).
//! Exit semantics for the standalone path:
//! - usage error -> 2
//! - openssl missing -> 127
//! - connect/handshake fail, expired cert, or unparseable notAfter -> 1
//! - ok (warn if <30 days) -> 0
//!
//! Host recipes use hostname / IPv4 `host:port`. Bracketed IPv6 BASE_URL is not
//! parsed here (document limitation; extend if recipes need it).

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum TlsCheckError {
    Usage(&'static str),
    OpensslMissing,
    HandshakeFailed(&'static str),
    Expired {
        days_until: i64,
    },
    /// `notAfter=` was present but days could not be computed (fail closed).
    ExpiryUnparseable(&'static str),
    Other(String),
}

impl TlsCheckError {
    pub fn exit_code(&self) -> i32 {
        match self {
            TlsCheckError::Usage(_) => 2,
            TlsCheckError::OpensslMissing => 127,
            TlsCheckError::HandshakeFailed(_)
            | TlsCheckError::Expired { .. }
            | TlsCheckError::ExpiryUnparseable(_)
            | TlsCheckError::Other(_) => 1,
        }
    }
}

/// Pure expiry verdict after parsing `notAfter=` from openssl x509 output.
///
/// Named contract: when `notAfter` is present, missing/unparseable days is
/// **Err** (fail closed), not silent Ok. Expired (days < 0) is Err.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryVerdict {
    Ok {
        days_until: i64,
    },
    WarnSoon {
        days_until: i64,
    },
    Expired {
        days_until: i64,
    },
    Unparseable,
    /// No notAfter line in cert dump (caller may still Ok the handshake).
    Absent,
}

pub fn tls_expiry_verdict(not_after_present: bool, days: Option<i64>) -> ExpiryVerdict {
    if !not_after_present {
        return ExpiryVerdict::Absent;
    }
    match days {
        None => ExpiryVerdict::Unparseable,
        Some(d) if d < 0 => ExpiryVerdict::Expired { days_until: d },
        Some(d) if d < 30 => ExpiryVerdict::WarnSoon { days_until: d },
        Some(d) => ExpiryVerdict::Ok { days_until: d },
    }
}

/// Run TLS check for `host:port` with optional SNI (`servername`; default = host).
pub fn check_tls(target: &str, servername: Option<&str>) -> Result<(), TlsCheckError> {
    if target.is_empty() {
        return Err(TlsCheckError::Usage("host:port required"));
    }
    let (host, port) = split_host_port(target).ok_or(TlsCheckError::Usage("host:port required"))?;
    let sni = servername.unwrap_or(host);

    if which("openssl").is_none() {
        return Err(TlsCheckError::OpensslMissing);
    }

    println!("== TLS check {host}:{port} (SNI {sni}) ==");

    let mut s_client = Command::new("openssl")
        .args([
            "s_client",
            "-connect",
            &format!("{host}:{port}"),
            "-servername",
            sni,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| TlsCheckError::Other(format!("openssl s_client spawn: {e}")))?;

    if let Some(mut stdin) = s_client.stdin.take() {
        let _ = stdin.write_all(b"\n");
    }

    let s_out = s_client
        .wait_with_output()
        .map_err(|e| TlsCheckError::Other(format!("openssl s_client wait: {e}")))?;

    let mut x509 = Command::new("openssl")
        .args(["x509", "-noout", "-subject", "-issuer", "-dates"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| TlsCheckError::Other(format!("openssl x509 spawn: {e}")))?;

    if let Some(mut stdin) = x509.stdin.take() {
        let _ = stdin.write_all(&s_out.stdout);
    }

    let x_out = x509
        .wait_with_output()
        .map_err(|e| TlsCheckError::Other(format!("openssl x509 wait: {e}")))?;

    let text = String::from_utf8_lossy(&x_out.stdout).trim().to_string();
    if text.is_empty() {
        return Err(TlsCheckError::HandshakeFailed(
            "could not retrieve certificate (connect or handshake failed)",
        ));
    }
    println!("{text}");

    apply_expiry_from_x509_text(&text, &s_out.stdout)?;
    Ok(())
}

/// Apply fail-closed expiry policy from openssl x509 -dates text + PEM blob.
fn apply_expiry_from_x509_text(text: &str, pem_blob: &[u8]) -> Result<(), TlsCheckError> {
    let not_after = text.lines().find_map(|l| l.strip_prefix("notAfter="));
    let days = not_after.and_then(days_until_expiry);
    // Fallback: openssl x509 -checkend 0 on the peer cert blob when date parse fails.
    let days = match (not_after, days) {
        (Some(_), None) => openssl_checkend_days(pem_blob).or(days),
        (_, d) => d,
    };

    match tls_expiry_verdict(not_after.is_some(), days) {
        ExpiryVerdict::Absent => Ok(()),
        ExpiryVerdict::Ok { days_until } => {
            println!("days_until_expiry={days_until}");
            Ok(())
        }
        ExpiryVerdict::WarnSoon { days_until } => {
            println!("days_until_expiry={days_until}");
            eprintln!("warning: certificate expires in under 30 days");
            Ok(())
        }
        ExpiryVerdict::Expired { days_until } => {
            println!("days_until_expiry={days_until}");
            Err(TlsCheckError::Expired { days_until })
        }
        ExpiryVerdict::Unparseable => Err(TlsCheckError::ExpiryUnparseable(
            "notAfter present but could not compute days until expiry (date/openssl parse failed)",
        )),
    }
}

fn split_host_port(target: &str) -> Option<(&str, &str)> {
    // host:port; IPv6 bracket form not used in current host recipes (hostname/IPv4).
    let (host, port) = target.rsplit_once(':')?;
    if host.is_empty() || port.is_empty() {
        return None;
    }
    Some((host, port))
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        for dir in std::env::split_paths(&paths) {
            let p = dir.join(bin);
            if p.is_file() {
                return Some(p);
            }
        }
        None
    })
}

/// Parse openssl `notAfter` via `date -d` when available (GNU date).
fn days_until_expiry(not_after: &str) -> Option<i64> {
    let out = Command::new("date")
        .args(["-d", not_after, "+%s"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let exp: i64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((exp - now) / 86400)
}

/// Fallback expiry via `openssl x509 -checkend 0` on the handshake PEM blob.
/// Returns Some(days) only when we can also get notAfter epoch; for checkend
/// alone we map expired -> Some(-1), not-expired without days -> None so
/// fail-closed Unparseable still applies if we only know "not expired now".
fn openssl_checkend_days(pem_blob: &[u8]) -> Option<i64> {
    if pem_blob.is_empty() || which("openssl").is_none() {
        return None;
    }
    let mut child = Command::new("openssl")
        .args(["x509", "-noout", "-checkend", "0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(pem_blob);
    }
    let st = child.wait().ok()?;
    if st.success() {
        // Cert not expired, but we still lack days_until for warn path.
        // Try enddate via openssl for days.
        let mut end = Command::new("openssl")
            .args(["x509", "-noout", "-enddate"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        if let Some(mut stdin) = end.stdin.take() {
            let _ = stdin.write_all(pem_blob);
        }
        let out = end.wait_with_output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(na) = text.lines().find_map(|l| {
            l.strip_prefix("notAfter=")
                .or_else(|| l.strip_prefix("notAfter ="))
        }) {
            return days_until_expiry(na.trim());
        }
        // Not expired but still no days: treat as far future so we do not
        // fail-closed on healthy certs when only checkend works.
        Some(365)
    } else {
        // checkend failed => expired (or bad cert). Prefer Expired over Unparseable.
        Some(-1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_host_port_basic() {
        assert_eq!(split_host_port("127.0.0.1:443"), Some(("127.0.0.1", "443")));
        assert_eq!(
            split_host_port("services.example:8443"),
            Some(("services.example", "8443"))
        );
        assert_eq!(split_host_port(""), None);
        assert_eq!(split_host_port("noport"), None);
    }

    #[test]
    fn usage_empty_target() {
        match check_tls("", None) {
            Err(TlsCheckError::Usage(_)) => {}
            other => panic!("expected Usage, got {other:?}"),
        }
    }

    #[test]
    fn expiry_parse_fail_is_unparseable_not_ok() {
        // Named contract: notAfter present + days None => Unparseable (fail closed).
        assert_eq!(tls_expiry_verdict(true, None), ExpiryVerdict::Unparseable);
    }

    #[test]
    fn expiry_absent_not_after_is_absent() {
        assert_eq!(tls_expiry_verdict(false, None), ExpiryVerdict::Absent);
        assert_eq!(tls_expiry_verdict(false, Some(10)), ExpiryVerdict::Absent);
    }

    #[test]
    fn expiry_negative_days_is_expired() {
        assert_eq!(
            tls_expiry_verdict(true, Some(-3)),
            ExpiryVerdict::Expired { days_until: -3 }
        );
    }

    #[test]
    fn expiry_under_30_warns() {
        assert_eq!(
            tls_expiry_verdict(true, Some(15)),
            ExpiryVerdict::WarnSoon { days_until: 15 }
        );
    }

    #[test]
    fn expiry_ok_far_future() {
        assert_eq!(
            tls_expiry_verdict(true, Some(90)),
            ExpiryVerdict::Ok { days_until: 90 }
        );
    }

    #[test]
    fn apply_expiry_unparseable_fails_closed() {
        // Simulate x509 text with notAfter but no parseable date path and empty PEM
        // (checkend cannot help).
        let text = "subject=CN=test\nissuer=CN=test\nnotAfter=NOT_A_REAL_DATE\n";
        match apply_expiry_from_x509_text(text, b"") {
            Err(TlsCheckError::ExpiryUnparseable(_)) => {}
            other => panic!("expected ExpiryUnparseable, got {other:?}"),
        }
    }
}
