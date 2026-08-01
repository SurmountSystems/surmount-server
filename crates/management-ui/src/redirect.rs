//! Pure helpers for HTTP -> HTTPS upgrade (edge :80 behavior).
//!
//! Product path: main binds a plain HTTP redirect-only listener when
//! `redirect_http_to_https` and `http_redirect_listen` are set (see main).
//! Dual-run nginx (`web.enable`) must not also own :80; Nix asserts that mutex.
//!
//! ACME HTTP-01 on product :80 is parked (Q-EDGE / residual). This path only
//! builds redirect targets and bind decisions.

use std::fmt;
use std::net::{IpAddr, SocketAddr};

/// Result of deciding whether / how to redirect plain HTTP to HTTPS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpToHttps {
    /// Redirect to this absolute HTTPS URL (typically 301 or 308).
    Redirect { location: String },
    /// Do not redirect (e.g. feature disabled or host not allowlisted).
    PassThrough,
}

/// Strip optional port from a Host header value.
/// Handles bracketed IPv6 (`[2001:db8::1]:8080` and `[2001:db8::1]`).
pub fn host_without_port(host: &str) -> &str {
    let host = host.trim();
    if host.is_empty() {
        return host;
    }
    if let Some(rest) = host.strip_prefix('[') {
        // [v6] or [v6]:port — `end` is index within `rest`; host has leading '['.
        if let Some(end) = rest.find(']') {
            return &host[..=end + 1];
        }
        return host;
    }
    // hostname:port or ipv4:port — only split on last colon if it looks like port
    if let Some((name, maybe_port)) = host.rsplit_once(':') {
        if maybe_port.chars().all(|c| c.is_ascii_digit()) && !maybe_port.is_empty() {
            // Avoid treating bare IPv6 without brackets as host:port (multiple colons).
            if name.contains(':') {
                return host;
            }
            return name;
        }
    }
    host
}

/// Inner host for URL authority: strip brackets from IPv6 literals.
pub fn host_for_url_authority(host_header: &str) -> String {
    let h = host_without_port(host_header);
    if let Some(inner) = h.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return inner.to_string();
    }
    h.to_string()
}

/// True when Host is allowed for redirects (exact match after port strip,
/// case-insensitive DNS labels; IPs compared canonically when both parse).
pub fn host_is_allowlisted(host_header: &str, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return false;
    }
    let candidate = host_for_url_authority(host_header);
    let cand_lower = candidate.to_ascii_lowercase();
    for a in allowed {
        let allow = host_for_url_authority(a);
        let allow_lower = allow.to_ascii_lowercase();
        if cand_lower == allow_lower {
            return true;
        }
        // Canonical IP compare when both parse as IPs.
        if let (Ok(c), Ok(al)) = (candidate.parse::<IpAddr>(), allow.parse::<IpAddr>()) {
            if c == al {
                return true;
            }
        }
    }
    false
}

/// Build an HTTPS Location URL from the original host + path + query.
///
/// `host` is the Host header without scheme (may include port).
/// When `allowed_hosts` is non-empty, Host must match or result is PassThrough.
/// When `https_port` is Some(443) or None, the port is omitted.
pub fn redirect_http_to_https(
    enabled: bool,
    host: &str,
    path_and_query: &str,
    https_port: Option<u16>,
    allowed_hosts: &[String],
) -> HttpToHttps {
    if !enabled {
        return HttpToHttps::PassThrough;
    }

    // Empty allowlist => no redirect (open-redirect safe default).
    if allowed_hosts.is_empty() || !host_is_allowlisted(host, allowed_hosts) {
        return HttpToHttps::PassThrough;
    }

    let host_only = host_for_url_authority(host);
    if host_only.is_empty() {
        return HttpToHttps::PassThrough;
    }

    let path = if path_and_query.is_empty() {
        "/"
    } else if path_and_query.starts_with('/') {
        path_and_query
    } else {
        return HttpToHttps::PassThrough;
    };

    // Bracket IPv6 in URL authority.
    let authority = if host_only
        .parse::<IpAddr>()
        .ok()
        .is_some_and(|ip| ip.is_ipv6())
    {
        format!("[{host_only}]")
    } else {
        host_only
    };

    let location = match https_port {
        None | Some(443) => format!("https://{authority}{path}"),
        Some(port) => format!("https://{authority}:{port}{path}"),
    };

    HttpToHttps::Redirect { location }
}

/// Prefer 308 for method-preserving upgrade; 301 also common for permanent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectStatus {
    PermanentRedirect308,
    /// Available for callers that want classic 301; product path uses 308.
    #[allow(dead_code)]
    MovedPermanently301,
}

impl RedirectStatus {
    pub fn as_u16(self) -> u16 {
        match self {
            RedirectStatus::PermanentRedirect308 => 308,
            RedirectStatus::MovedPermanently301 => 301,
        }
    }
}

impl fmt::Display for RedirectStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u16())
    }
}

/// Whether main should bind the plain HTTP redirect-only listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectBindDecision {
    /// Bind redirect-only plain HTTP at this address.
    Bind { addr: SocketAddr },
    /// Feature off or listen unset: do not bind (not an error).
    Skip { reason: &'static str },
    /// Misconfiguration: refuse start (open-redirect-safe refuse).
    FailClosed { reason: &'static str },
}

/// Decide redirect listener bind from config flags.
///
/// - Flag off => Skip (no bind).
/// - Flag on, listen unset => Skip (no bind; operator left listen empty).
/// - Flag on, listen set, empty allowlist => FailClosed (would never redirect
///   and could look like a half-wired cleartext port).
/// - Flag on, listen set, non-empty allowlist => Bind.
///
/// Dual-run nginx ownership of :80 is enforced in the Nix module (eval mutex),
/// not here (binary has no web.enable knowledge).
pub fn redirect_bind_decision(
    enabled: bool,
    listen: Option<SocketAddr>,
    allowed_hosts: &[String],
) -> RedirectBindDecision {
    if !enabled {
        return RedirectBindDecision::Skip {
            reason: "redirect_http_to_https off",
        };
    }
    let Some(addr) = listen else {
        return RedirectBindDecision::Skip {
            reason: "http_redirect_listen unset",
        };
    };
    if allowed_hosts.is_empty() {
        return RedirectBindDecision::FailClosed {
            reason: "redirect_http_to_https on but redirect allowlist is empty \
                     (open-redirect-safe refuse; set SURMOUNT_REDIRECT_ALLOWED_HOSTS \
                     or services/primary hostnames)",
        };
    }
    RedirectBindDecision::Bind { addr }
}

/// HTTPS port to embed in Location when upgrading from plain HTTP.
/// Omits 443 (None); includes other ports.
pub fn https_port_for_redirect(https_listen_port: u16) -> Option<u16> {
    match https_listen_port {
        443 => None,
        p => Some(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(hosts: &[&str]) -> Vec<String> {
        hosts.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn disabled_is_pass_through() {
        assert_eq!(
            redirect_http_to_https(
                false,
                "services.example.test",
                "/health",
                None,
                &allow(&["services.example.test"])
            ),
            HttpToHttps::PassThrough
        );
    }

    #[test]
    fn default_https_omits_port_443() {
        let a = allow(&["services.example.test"]);
        assert_eq!(
            redirect_http_to_https(true, "services.example.test", "/health", None, &a),
            HttpToHttps::Redirect {
                location: "https://services.example.test/health".into()
            }
        );
        assert_eq!(
            redirect_http_to_https(true, "services.example.test:80", "/x?y=1", Some(443), &a),
            HttpToHttps::Redirect {
                location: "https://services.example.test/x?y=1".into()
            }
        );
    }

    #[test]
    fn nonstandard_https_port_is_included() {
        assert_eq!(
            redirect_http_to_https(true, "localhost", "/", Some(8443), &allow(&["localhost"])),
            HttpToHttps::Redirect {
                location: "https://localhost:8443/".into()
            }
        );
    }

    #[test]
    fn empty_path_becomes_root() {
        assert_eq!(
            redirect_http_to_https(true, "example.test", "", None, &allow(&["example.test"])),
            HttpToHttps::Redirect {
                location: "https://example.test/".into()
            }
        );
    }

    #[test]
    fn empty_host_is_pass_through() {
        assert_eq!(
            redirect_http_to_https(true, "", "/a", None, &allow(&["example.test"])),
            HttpToHttps::PassThrough
        );
    }

    #[test]
    fn host_not_allowlisted_is_pass_through() {
        assert_eq!(
            redirect_http_to_https(
                true,
                "evil.example",
                "/",
                None,
                &allow(&["services.example.test"])
            ),
            HttpToHttps::PassThrough
        );
    }

    #[test]
    fn empty_allowlist_blocks_redirect() {
        // Open-redirect safe default: no hosts configured => no redirect.
        assert_eq!(
            redirect_http_to_https(true, "services.example.test", "/", None, &[]),
            HttpToHttps::PassThrough
        );
    }

    #[test]
    fn ipv6_host_header_with_port() {
        let a = allow(&["2001:db8::1"]);
        assert_eq!(host_without_port("[2001:db8::1]:8080"), "[2001:db8::1]");
        assert_eq!(
            redirect_http_to_https(true, "[2001:db8::1]:80", "/h", None, &a),
            HttpToHttps::Redirect {
                location: "https://[2001:db8::1]/h".into()
            }
        );
    }

    #[test]
    fn redirect_status_codes() {
        assert_eq!(RedirectStatus::PermanentRedirect308.as_u16(), 308);
        assert_eq!(RedirectStatus::MovedPermanently301.as_u16(), 301);
    }

    #[test]
    fn bind_decision_skips_when_flag_off() {
        let addr = "0.0.0.0:80".parse().unwrap();
        assert_eq!(
            redirect_bind_decision(false, Some(addr), &allow(&["a.test"])),
            RedirectBindDecision::Skip {
                reason: "redirect_http_to_https off"
            }
        );
    }

    #[test]
    fn bind_decision_skips_when_listen_unset() {
        assert_eq!(
            redirect_bind_decision(true, None, &allow(&["a.test"])),
            RedirectBindDecision::Skip {
                reason: "http_redirect_listen unset"
            }
        );
    }

    #[test]
    fn bind_decision_fail_closed_empty_allowlist() {
        let addr = "127.0.0.1:8080".parse().unwrap();
        match redirect_bind_decision(true, Some(addr), &[]) {
            RedirectBindDecision::FailClosed { reason } => {
                assert!(reason.contains("allowlist"), "{reason}");
            }
            other => panic!("expected FailClosed, got {other:?}"),
        }
    }

    #[test]
    fn bind_decision_bind_when_enabled_listen_and_hosts() {
        let addr: SocketAddr = "0.0.0.0:80".parse().unwrap();
        assert_eq!(
            redirect_bind_decision(true, Some(addr), &allow(&["services.example.test"])),
            RedirectBindDecision::Bind { addr }
        );
    }

    #[test]
    fn https_port_for_redirect_omits_443() {
        assert_eq!(https_port_for_redirect(443), None);
        assert_eq!(https_port_for_redirect(8443), Some(8443));
    }
}
