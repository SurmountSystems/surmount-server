//! D1 hybrid TLS negotiation probe. Env-gated. Never silent skip success.

use std::process::Command;

use crate::ProbeError;

pub const USAGE: &str = "\
Usage: SURMOUNT_E2E_BASE_URL=https://host[:port][/path] surmount-tls-hybrid

D1 hybrid TLS negotiation probe after B1 public HTTPS.

  SURMOUNT_E2E_BASE_URL     required (https only; env placeholder, never commit)
  SURMOUNT_TLS_SERVERNAME   optional SNI (default: host from BASE_URL)

Exit 2 when BASE_URL is unset (BLOCKED). Local hermetic offline:
  cargo test -p surmount-management-ui --bin surmount-management-ui tls::
";

pub const BLOCKED_MSG: &str = "\
error: BLOCKED: SURMOUNT_E2E_BASE_URL is unset

  surmount-tls-hybrid is a host hybrid negotiation probe. It refuses
  to mark hybrid TLS as proven when BASE_URL is missing (no silent skip
  success / no false-green cutover).

  After B1 public HTTPS (operator; placeholders only):

    export SURMOUNT_E2E_BASE_URL=https://example.test:443
    # optional SNI:
    # export SURMOUNT_TLS_SERVERNAME=example.test
    nix run .#surmount-tls-hybrid

  Offline provider proof (not host negotiation):

    cd crates && cargo test -p surmount-management-ui --bin surmount-management-ui tls::

  See docs/deploy-host-local.md (D1), docs/EDGE_AND_TLS.md, RESIDUAL.md.
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connect {
    pub host: String,
    pub port: u16,
    pub connect: String,
    pub servername: String,
}

pub fn parse_https_base(url: &str, sni_override: Option<&str>) -> Result<Connect, ProbeError> {
    if url.starts_with('-') {
        return Err(ProbeError::fail(
            "SURMOUNT_E2E_BASE_URL must not start with '-' (option-shaped)",
        ));
    }
    let rest = url.strip_prefix("https://").ok_or_else(|| {
        ProbeError::fail("SURMOUNT_E2E_BASE_URL must be https://... (got non-https scheme)")
    })?;
    let rest = rest.split(['/', '?', '#']).next().unwrap_or("");
    if rest.is_empty() {
        return Err(ProbeError::fail("SURMOUNT_E2E_BASE_URL has empty host"));
    }
    let (host, port) = if rest.starts_with('[') {
        if let Some(end) = rest.find(']') {
            let host = rest[1..end].to_string();
            let port = if rest[end + 1..].starts_with(':') {
                parse_port(&rest[end + 2..])?
            } else {
                443
            };
            (host, port)
        } else {
            return Err(ProbeError::fail(
                "could not parse IPv6 host from SURMOUNT_E2E_BASE_URL",
            ));
        }
    } else if let Some((h, p)) = rest.rsplit_once(':') {
        (h.to_string(), parse_port(p)?)
    } else {
        (rest.to_string(), 443)
    };
    if host.is_empty() {
        return Err(ProbeError::fail(
            "empty host after parsing SURMOUNT_E2E_BASE_URL",
        ));
    }
    if host.starts_with('-') {
        return Err(ProbeError::fail(
            "host or port must not start with '-' (option-shaped)",
        ));
    }
    let servername = sni_override
        .filter(|s| !s.is_empty())
        .unwrap_or(host.as_str())
        .to_string();
    if servername.starts_with('-') {
        return Err(ProbeError::fail(
            "SURMOUNT_TLS_SERVERNAME must not start with '-'",
        ));
    }
    let connect = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    Ok(Connect {
        host,
        port,
        connect,
        servername,
    })
}

fn parse_port(p: &str) -> Result<u16, ProbeError> {
    if p.starts_with('-') {
        return Err(ProbeError::fail(
            "host or port must not start with '-' (option-shaped)",
        ));
    }
    let n: u32 = p
        .parse()
        .map_err(|_| ProbeError::fail(format!("invalid port '{p}'")))?;
    if !(1..=65535).contains(&n) {
        return Err(ProbeError::fail(format!("invalid port '{p}'")));
    }
    Ok(n as u16)
}

pub fn negotiated_group(openssl_out: &str) -> Option<String> {
    for line in openssl_out.lines() {
        if let Some(rest) = line.strip_prefix("Negotiated TLS1.3 group:") {
            let g = rest.trim();
            if !g.is_empty() {
                return Some(g.to_string());
            }
        }
        if let Some(rest) = line.strip_prefix("Negotiated TLS") {
            if let Some((_, g)) = rest.split_once("group:") {
                let g = g.trim();
                if !g.is_empty() {
                    return Some(g.to_string());
                }
            }
        }
    }
    None
}

pub fn protocol_line(openssl_out: &str) -> Option<String> {
    openssl_out
        .lines()
        .find_map(|l| l.strip_prefix("Protocol:").map(|s| s.trim().to_string()))
}

pub fn classify_group(group: &str) -> bool {
    group.to_ascii_uppercase().contains("MLKEM")
}

pub fn openssl_has_groups(help: &str) -> bool {
    help.contains("-groups")
}

pub fn run_probe() -> Result<i32, ProbeError> {
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("-h") | Some("--help")
    ) {
        print!("{USAGE}");
        return Ok(0);
    }
    let base = std::env::var("SURMOUNT_E2E_BASE_URL").unwrap_or_default();
    if base.is_empty() {
        eprint!("{BLOCKED_MSG}");
        return Ok(2);
    }
    let sni = std::env::var("SURMOUNT_TLS_SERVERNAME").ok();
    let conn = parse_https_base(&base, sni.as_deref())?;
    let openssl = crate::resolve_program("openssl", "openssl").map_err(|_| ProbeError {
        message: "openssl not found on PATH (required for hybrid probe)".into(),
        exit_code: 127,
    })?;
    let ver = Command::new(&openssl)
        .arg("version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("== D1 hybrid TLS probe ==");
    println!("base_url={base}");
    println!("connect={}", conn.connect);
    println!("sni={}", conn.servername);
    println!("openssl={}", ver.lines().next().unwrap_or(""));

    let help = Command::new(&openssl)
        .args(["s_client", "-help"])
        .output()
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .unwrap_or_default();
    if !openssl_has_groups(&help) {
        return Err(ProbeError::fail(
            "cannot measure: this openssl s_client has no -groups (too old?)",
        ));
    }
    let groups = "X25519MLKEM768:SecP256r1MLKEM768:X25519:secp256r1";
    let out = Command::new(&openssl)
        .args([
            "s_client",
            "-connect",
            &conn.connect,
            "-servername",
            &conn.servername,
            "-tls1_3",
            "-groups",
            groups,
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| ProbeError::fail(format!("openssl s_client failed: {e}")))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if let Some(p) = protocol_line(&text) {
        println!("protocol={p}");
    } else {
        println!("protocol=(unparsed)");
    }
    if let Some(g) = negotiated_group(&text) {
        println!("negotiated_group={g}");
        if classify_group(&g) {
            println!("result=HYBRID");
            println!("ok: hybrid key exchange negotiated ({g})");
            return Ok(0);
        }
        println!("result=CLASSICAL_ONLY");
        eprintln!("error: classical-only negotiated group: {g}");
        eprintln!("\n  Handshake worked, but this probe did not negotiate a hybrid (MLKEM) group.");
        return Ok(1);
    }
    if !out.status.success() {
        return Err(ProbeError::fail(format!(
            "TLS handshake failed (openssl exit {}); cannot measure group",
            out.status.code().unwrap_or(1)
        )));
    }
    Err(ProbeError::fail(
        "cannot measure: openssl did not print a negotiated TLS 1.3 group line",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_host_port() {
        let c = parse_https_base("https://example.test:8443/x", None).unwrap();
        assert_eq!(c.host, "example.test");
        assert_eq!(c.port, 8443);
        assert_eq!(c.connect, "example.test:8443");
    }

    #[test]
    fn http_refused() {
        assert!(parse_https_base("http://example.test", None).is_err());
    }

    #[test]
    fn option_shaped_url() {
        assert!(parse_https_base("-oProxyCommand=true", None).is_err());
    }

    #[test]
    fn mlkem_is_hybrid() {
        assert!(classify_group("X25519MLKEM768"));
        assert!(!classify_group("X25519"));
    }

    #[test]
    fn parse_negotiated_line() {
        let out = "Protocol: TLSv1.3\nNegotiated TLS1.3 group: X25519MLKEM768\n";
        assert_eq!(negotiated_group(out).as_deref(), Some("X25519MLKEM768"));
    }
}
