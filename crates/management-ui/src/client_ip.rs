//! Client identity for edge rate limiting.
//!
//! Dual-run (nginx -> loopback UI): peer is always 127.0.0.1, so we only trust
//! X-Real-IP when the TCP peer is loopback. X-Forwarded-For is not used for
//! rate-limit keys (leftmost XFF is spoofable by the client).
//! Never trust forwarding headers from non-loopback peers.

use std::net::IpAddr;

/// Resolve the rate-limit key for a request.
///
/// Policy:
/// - If `peer` is loopback and `x_real_ip` parses as an IP, use it.
/// - Else use `peer` (string form).
/// - `x_forwarded_for` is ignored (spoofable leftmost hop).
pub fn client_ip_for_rate_limit(
    peer: IpAddr,
    x_real_ip: Option<&str>,
    _x_forwarded_for: Option<&str>,
) -> String {
    if peer.is_loopback()
        && let Some(ip) = x_real_ip.and_then(parse_single_ip)
    {
        return ip.to_string();
    }
    peer.to_string()
}

fn parse_single_ip(raw: &str) -> Option<IpAddr> {
    raw.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn non_loopback_ignores_forwarding_headers() {
        let peer = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));
        let key = client_ip_for_rate_limit(peer, Some("10.0.0.1"), Some("10.0.0.2"));
        assert_eq!(key, "203.0.113.9");
    }

    #[test]
    fn loopback_prefers_x_real_ip() {
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let key = client_ip_for_rate_limit(peer, Some("198.51.100.7"), Some("203.0.113.1"));
        assert_eq!(key, "198.51.100.7");
    }

    #[test]
    fn loopback_xff_alone_does_not_spoof_key() {
        // Contract: leftmost XFF without X-Real-IP must not become the key.
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let key = client_ip_for_rate_limit(peer, None, Some("evil, 198.51.100.1"));
        assert_ne!(key, "evil");
        assert_ne!(key, "198.51.100.1");
        assert_eq!(key, "127.0.0.1");
    }

    #[test]
    fn loopback_falls_back_to_peer_without_headers() {
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(client_ip_for_rate_limit(peer, None, None), "127.0.0.1");
    }

    #[test]
    fn loopback_ipv6_trusts_x_real_ip() {
        let peer = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let key = client_ip_for_rate_limit(peer, Some("2001:db8::1"), None);
        assert_eq!(key, "2001:db8::1");
    }

    #[test]
    fn garbage_headers_fall_back_to_peer() {
        let peer = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(
            client_ip_for_rate_limit(peer, Some("not-an-ip"), Some("also-bad")),
            "127.0.0.1"
        );
    }
}
