//! Pure helpers for HTTP -> HTTPS upgrade (edge :80 behavior).
//!
//! Product path: main binds a plain HTTP redirect-only listener when
//! `redirect_http_to_https` and `http_redirect_listen` are set (see main).
//! Dual-run nginx (`web.enable`) must not also own :80; Nix asserts that mutex.
//!
//! ACME HTTP-01 on product :80 is parked (Q-EDGE / residual). This path only
//! builds redirect targets and bind decisions.

use std::collections::BTreeMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

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
    if let Some((name, maybe_port)) = host.rsplit_once(':')
        && maybe_port.chars().all(|c| c.is_ascii_digit())
        && !maybe_port.is_empty()
    {
        // Avoid treating bare IPv6 without brackets as host:port (multiple colons).
        if name.contains(':') {
            return host;
        }
        return name;
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
        if let (Ok(c), Ok(al)) = (candidate.parse::<IpAddr>(), allow.parse::<IpAddr>())
            && c == al
        {
            return true;
        }
    }
    false
}

/// True when Host is the apex primary domain or `www.` of that domain.
///
/// Case-insensitive DNS labels; port stripped. Empty primary => never true.
/// Product: apex/www are the public main site (COMING SOON), not the operator
/// console. Console lives only on the services hostname.
pub fn is_apex_or_www(host_header: &str, primary_domain: &str) -> bool {
    let primary = host_for_url_authority(primary_domain);
    if primary.is_empty() {
        return false;
    }
    let h = host_for_url_authority(host_header).to_ascii_lowercase();
    let p = primary.to_ascii_lowercase();
    h == p || h == format!("www.{p}")
}

/// True when Host is the operator services hostname (console).
///
/// Case-insensitive DNS labels; port stripped. Empty services => never true.
pub fn is_services_host(host_header: &str, services_hostname: &str) -> bool {
    let services = host_for_url_authority(services_hostname);
    if services.is_empty() {
        return false;
    }
    host_for_url_authority(host_header).eq_ignore_ascii_case(&services)
}

/// Which public product surface a request Host maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostSurface {
    /// Apex or `www.<primary>`: public COMING SOON (not operator console).
    ApexPublic,
    /// Extra static Host with its own document root (not apex/www, not services).
    StaticVhost,
    /// Services hostname: operator management console.
    ServicesConsole,
    /// Other Host (mail, mta-sts, lab IP, empty, unknown): no public remap.
    Other,
}

/// Host used for product surface routing (apex COMING SOON vs services console).
///
/// Prefer a non-empty `Host` header (HTTP/1.1). When it is missing or blank,
/// use the request URI host so HTTP/2 `:authority` (hyper stores it on the
/// URI) still classifies apex/www. Empty both => empty string (Other surface).
pub fn request_authority_host(host_header: Option<&str>, uri_host: Option<&str>) -> String {
    if let Some(h) = host_header.map(str::trim).filter(|s| !s.is_empty()) {
        return h.to_string();
    }
    uri_host
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Classify Host for product routing (apex public vs services console).
///
/// When Host is both apex-shaped and equal to services (mis-set config),
/// ServicesConsole wins so the operator console is not replaced by COMING SOON.
pub fn host_surface(
    host_header: &str,
    primary_domain: &str,
    services_hostname: &str,
) -> HostSurface {
    classify_host_surface(
        host_header,
        primary_domain,
        services_hostname,
        &BTreeMap::new(),
    )
}

/// Classify Host including extra static vhosts (Host -> document root map).
///
/// Services wins over everything. Apex/www stay ApexPublic even if listed in
/// the extra map (do not overload those names). Unknown Host stays Other
/// (no static leak onto the console).
pub fn classify_host_surface(
    host_header: &str,
    primary_domain: &str,
    services_hostname: &str,
    static_vhosts: &BTreeMap<String, PathBuf>,
) -> HostSurface {
    if is_services_host(host_header, services_hostname) {
        return HostSurface::ServicesConsole;
    }
    if is_apex_or_www(host_header, primary_domain) {
        return HostSurface::ApexPublic;
    }
    if static_vhost_mapped(host_header, static_vhosts) {
        return HostSurface::StaticVhost;
    }
    HostSurface::Other
}

/// True when Host (port stripped, case-insensitive) is a configured extra vhost.
pub fn static_vhost_mapped(host_header: &str, static_vhosts: &BTreeMap<String, PathBuf>) -> bool {
    if static_vhosts.is_empty() {
        return false;
    }
    let host = host_for_url_authority(host_header).to_ascii_lowercase();
    !host.is_empty() && static_vhosts.contains_key(&host)
}

/// Document root for an extra static Host, if mapped.
pub fn static_vhost_document_root<'a>(
    host_header: &str,
    static_vhosts: &'a BTreeMap<String, PathBuf>,
) -> Option<&'a std::path::Path> {
    let host = host_for_url_authority(host_header).to_ascii_lowercase();
    static_vhosts.get(&host).map(PathBuf::as_path)
}

/// True when this surface serves public static files (apex site or extra vhost).
pub fn is_public_static_surface(surface: HostSurface) -> bool {
    matches!(surface, HostSurface::ApexPublic | HostSurface::StaticVhost)
}

/// Paths that stay available on apex/www (health probes, ACME/MTA-STS well-known).
///
/// Everything else on apex/www is the public COMING SOON page (HTML) or a
/// closed API 404; never the operator console.
pub fn apex_public_path_is_edge_exception(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    path == "/health" || path == "/api/health" || path.starts_with("/.well-known/")
}

/// True when the edge should short-circuit to the public COMING SOON HTML page.
///
/// Requires ApexPublic surface and a non-exception path. API paths on apex are
/// handled separately (404), not COMING SOON HTML.
pub fn should_serve_apex_coming_soon(
    host_header: &str,
    primary_domain: &str,
    services_hostname: &str,
    path: &str,
) -> bool {
    if host_surface(host_header, primary_domain, services_hostname) != HostSurface::ApexPublic {
        return false;
    }
    let path_only = path.split('?').next().unwrap_or(path);
    if apex_public_path_is_edge_exception(path_only) {
        return false;
    }
    // JSON/API on apex: not the public HTML page (caller returns 404).
    if path_only.starts_with("/api/") {
        return false;
    }
    true
}

/// True when apex/www Host should get a closed (non-console) API response.
pub fn should_reject_apex_api(
    host_header: &str,
    primary_domain: &str,
    services_hostname: &str,
    path: &str,
) -> bool {
    should_reject_public_static_api(
        host_header,
        primary_domain,
        services_hostname,
        &BTreeMap::new(),
        path,
    )
}

/// True when apex/www or an extra static Host should get a closed API 404.
pub fn should_reject_public_static_api(
    host_header: &str,
    primary_domain: &str,
    services_hostname: &str,
    static_vhosts: &BTreeMap<String, PathBuf>,
    path: &str,
) -> bool {
    if !is_public_static_surface(classify_host_surface(
        host_header,
        primary_domain,
        services_hostname,
        static_vhosts,
    )) {
        return false;
    }
    let path_only = path.split('?').next().unwrap_or(path);
    path_only.starts_with("/api/") && !apex_public_path_is_edge_exception(path_only)
}

/// Location authority for an allowlisted Host (same-host HTTPS upgrade).
///
/// Apex and www keep their own Host so public users land on the main site
/// (COMING SOON), not `services.*`. Operator console is not the HTTP->HTTPS
/// target for apex/www. Open-redirect safe: caller must still allowlist-check
/// the request Host.
///
/// `primary_domain` and `services_hostname` are retained for call-site
/// compatibility; they no longer remap authority (historical R1/R2 remap
/// removed 2026-08-11: public apex must not dump into the operator console).
pub fn redirect_location_authority(
    host_header: &str,
    _primary_domain: &str,
    _services_hostname: &str,
) -> String {
    host_for_url_authority(host_header)
}

/// Build `https://{authority}{path}` with optional non-443 port; bracket IPv6.
pub fn https_location(authority_host: &str, path: &str, https_port: Option<u16>) -> String {
    let authority = if authority_host
        .parse::<IpAddr>()
        .ok()
        .is_some_and(|ip| ip.is_ipv6())
    {
        format!("[{authority_host}]")
    } else {
        authority_host.to_string()
    };
    match https_port {
        None | Some(443) => format!("https://{authority}{path}"),
        Some(port) => format!("https://{authority}:{port}{path}"),
    }
}

/// Normalize path+query for Location: empty -> `/`; must be absolute path.
fn normalize_redirect_path(path_and_query: &str) -> Option<&str> {
    if path_and_query.is_empty() {
        Some("/")
    } else if path_and_query.starts_with('/') {
        Some(path_and_query)
    } else {
        None
    }
}

/// Build an HTTPS Location URL from the original host + path + query.
///
/// `host` is the Host header without scheme (may include port).
/// When `allowed_hosts` is non-empty, Host must match or result is PassThrough.
/// When `https_port` is Some(443) or None, the port is omitted.
///
/// Same-host upgrade for every allowlisted Host, including apex and www.
/// Public users on apex/www upgrade to `https://{apex|www}/...` (COMING SOON
/// on the primary edge), never to the services operator console.
pub fn redirect_http_to_https(
    enabled: bool,
    host: &str,
    path_and_query: &str,
    https_port: Option<u16>,
    allowed_hosts: &[String],
    primary_domain: &str,
    services_hostname: &str,
) -> HttpToHttps {
    if !enabled {
        return HttpToHttps::PassThrough;
    }

    // Empty allowlist => no redirect (open-redirect safe default).
    if allowed_hosts.is_empty() || !host_is_allowlisted(host, allowed_hosts) {
        return HttpToHttps::PassThrough;
    }

    let host_only = redirect_location_authority(host, primary_domain, services_hostname);
    if host_only.is_empty() {
        return HttpToHttps::PassThrough;
    }

    let Some(path) = normalize_redirect_path(path_and_query) else {
        return HttpToHttps::PassThrough;
    };

    HttpToHttps::Redirect {
        location: https_location(&host_only, path, https_port),
    }
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

    /// Same-host HTTPS upgrade helper (primary/services unused for authority).
    fn upgrade(
        enabled: bool,
        host: &str,
        path: &str,
        port: Option<u16>,
        allowed: &[String],
    ) -> HttpToHttps {
        redirect_http_to_https(enabled, host, path, port, allowed, "", "")
    }

    #[test]
    fn disabled_is_pass_through() {
        assert_eq!(
            upgrade(
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
            upgrade(true, "services.example.test", "/health", None, &a),
            HttpToHttps::Redirect {
                location: "https://services.example.test/health".into()
            }
        );
        assert_eq!(
            upgrade(true, "services.example.test:80", "/x?y=1", Some(443), &a),
            HttpToHttps::Redirect {
                location: "https://services.example.test/x?y=1".into()
            }
        );
    }

    #[test]
    fn nonstandard_https_port_is_included() {
        assert_eq!(
            upgrade(true, "localhost", "/", Some(8443), &allow(&["localhost"])),
            HttpToHttps::Redirect {
                location: "https://localhost:8443/".into()
            }
        );
    }

    #[test]
    fn empty_path_becomes_root() {
        assert_eq!(
            upgrade(true, "example.test", "", None, &allow(&["example.test"])),
            HttpToHttps::Redirect {
                location: "https://example.test/".into()
            }
        );
    }

    #[test]
    fn empty_host_is_pass_through() {
        assert_eq!(
            upgrade(true, "", "/a", None, &allow(&["example.test"])),
            HttpToHttps::PassThrough
        );
    }

    #[test]
    fn host_not_allowlisted_is_pass_through() {
        assert_eq!(
            upgrade(
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
            upgrade(true, "services.example.test", "/", None, &[]),
            HttpToHttps::PassThrough
        );
    }

    #[test]
    fn ipv6_host_header_with_port() {
        let a = allow(&["2001:db8::1"]);
        assert_eq!(host_without_port("[2001:db8::1]:8080"), "[2001:db8::1]");
        assert_eq!(
            upgrade(true, "[2001:db8::1]:80", "/h", None, &a),
            HttpToHttps::Redirect {
                location: "https://[2001:db8::1]/h".into()
            }
        );
    }

    /// Named contract: apex HTTP->HTTPS is same-host (public site), not services.
    #[test]
    fn apex_http_upgrade_is_same_host_not_services() {
        let a = allow(&["example.test", "www.example.test", "services.example.test"]);
        assert_eq!(
            redirect_http_to_https(
                true,
                "example.test",
                "/health",
                None,
                &a,
                "example.test",
                "services.example.test",
            ),
            HttpToHttps::Redirect {
                location: "https://example.test/health".into()
            }
        );
        // Path and query preserved; non-443 primary port included.
        assert_eq!(
            redirect_http_to_https(
                true,
                "example.test:80",
                "/x?y=1",
                Some(8443),
                &a,
                "example.test",
                "services.example.test",
            ),
            HttpToHttps::Redirect {
                location: "https://example.test:8443/x?y=1".into()
            }
        );
    }

    /// Named contract: www HTTP->HTTPS is same-host (public site), not services.
    #[test]
    fn www_http_upgrade_is_same_host_not_services() {
        let a = allow(&["www.example.test", "services.example.test", "example.test"]);
        assert_eq!(
            redirect_http_to_https(
                true,
                "www.example.test",
                "/",
                None,
                &a,
                "example.test",
                "services.example.test",
            ),
            HttpToHttps::Redirect {
                location: "https://www.example.test/".into()
            }
        );
        // Case-insensitive allowlist match; Location keeps request Host spelling.
        assert_eq!(
            redirect_http_to_https(
                true,
                "WWW.Example.TEST",
                "/login",
                None,
                &a,
                "example.test",
                "services.example.test",
            ),
            HttpToHttps::Redirect {
                location: "https://WWW.Example.TEST/login".into()
            }
        );
    }

    #[test]
    fn services_host_keeps_same_host_upgrade() {
        let a = allow(&["services.example.test", "example.test"]);
        assert_eq!(
            redirect_http_to_https(
                true,
                "services.example.test",
                "/health",
                None,
                &a,
                "example.test",
                "services.example.test",
            ),
            HttpToHttps::Redirect {
                location: "https://services.example.test/health".into()
            }
        );
    }

    #[test]
    fn apex_not_allowlisted_never_redirects_even_with_services() {
        // Open-redirect: apex must be on the allowlist, not only services.
        assert_eq!(
            redirect_http_to_https(
                true,
                "example.test",
                "/",
                None,
                &allow(&["services.example.test"]),
                "example.test",
                "services.example.test",
            ),
            HttpToHttps::PassThrough
        );
    }

    /// Named contract: Host surface routing (apex public vs services console).
    #[test]
    fn host_surface_apex_www_services_and_other() {
        assert_eq!(
            host_surface("example.test", "example.test", "services.example.test"),
            HostSurface::ApexPublic
        );
        assert_eq!(
            host_surface("www.example.test", "example.test", "services.example.test"),
            HostSurface::ApexPublic
        );
        assert_eq!(
            host_surface("WWW.Example.TEST", "example.test", "services.example.test"),
            HostSurface::ApexPublic
        );
        assert_eq!(
            host_surface(
                "services.example.test",
                "example.test",
                "services.example.test"
            ),
            HostSurface::ServicesConsole
        );
        // Mis-set: services equal primary => services wins (console not replaced).
        assert_eq!(
            host_surface("example.test", "example.test", "example.test"),
            HostSurface::ServicesConsole
        );
        assert_eq!(
            host_surface("mail.example.test", "example.test", "services.example.test"),
            HostSurface::Other
        );
        assert_eq!(
            host_surface("evil.example", "example.test", "services.example.test"),
            HostSurface::Other
        );
    }

    /// Named contract: extra static Host is StaticVhost; services/apex win;
    /// unknown Host is Other (no leak).
    #[test]
    fn host_surface_static_vhost_not_apex_not_services() {
        let mut map = BTreeMap::new();
        map.insert(
            "extra.test".into(),
            std::path::PathBuf::from("/var/lib/surmount/static-sites/extra"),
        );
        map.insert(
            "www.extra.test".into(),
            std::path::PathBuf::from("/var/lib/surmount/static-sites/extra"),
        );
        assert_eq!(
            classify_host_surface("extra.test", "example.test", "services.example.test", &map),
            HostSurface::StaticVhost
        );
        assert_eq!(
            classify_host_surface(
                "www.extra.test",
                "example.test",
                "services.example.test",
                &map
            ),
            HostSurface::StaticVhost
        );
        assert_eq!(
            classify_host_surface(
                "EXTRA.test:443",
                "example.test",
                "services.example.test",
                &map
            ),
            HostSurface::StaticVhost
        );
        // Apex stays ApexPublic even if someone listed it in the extra map.
        map.insert(
            "example.test".into(),
            std::path::PathBuf::from("/var/lib/surmount/static-sites/wrong"),
        );
        assert_eq!(
            classify_host_surface(
                "example.test",
                "example.test",
                "services.example.test",
                &map
            ),
            HostSurface::ApexPublic
        );
        assert_eq!(
            classify_host_surface(
                "services.example.test",
                "example.test",
                "services.example.test",
                &map
            ),
            HostSurface::ServicesConsole
        );
        assert_eq!(
            classify_host_surface(
                "unknown.example",
                "example.test",
                "services.example.test",
                &map
            ),
            HostSurface::Other
        );
        assert!(should_reject_public_static_api(
            "extra.test",
            "example.test",
            "services.example.test",
            &map,
            "/api/v1/domains"
        ));
        assert!(!should_reject_public_static_api(
            "unknown.example",
            "example.test",
            "services.example.test",
            &map,
            "/api/v1/domains"
        ));
    }

    /// Named contract: apex COMING SOON for public paths; health/API exceptions.
    #[test]
    fn apex_coming_soon_and_api_reject_path_rules() {
        let primary = "example.test";
        let services = "services.example.test";
        assert!(should_serve_apex_coming_soon(
            "example.test",
            primary,
            services,
            "/"
        ));
        assert!(should_serve_apex_coming_soon(
            "www.example.test",
            primary,
            services,
            "/domains"
        ));
        assert!(should_serve_apex_coming_soon(
            "example.test",
            primary,
            services,
            "/login"
        ));
        // Health stays for probes.
        assert!(!should_serve_apex_coming_soon(
            "example.test",
            primary,
            services,
            "/health"
        ));
        assert!(!should_serve_apex_coming_soon(
            "example.test",
            primary,
            services,
            "/api/health"
        ));
        // Services never gets COMING SOON gate.
        assert!(!should_serve_apex_coming_soon(
            "services.example.test",
            primary,
            services,
            "/"
        ));
        // API on apex is rejected, not COMING SOON HTML.
        assert!(!should_serve_apex_coming_soon(
            "example.test",
            primary,
            services,
            "/api/v1/domains"
        ));
        assert!(should_reject_apex_api(
            "example.test",
            primary,
            services,
            "/api/v1/domains"
        ));
        assert!(!should_reject_apex_api(
            "services.example.test",
            primary,
            services,
            "/api/v1/domains"
        ));
        assert!(!should_reject_apex_api(
            "example.test",
            primary,
            services,
            "/api/health"
        ));
    }

    /// Named contract: empty Host header falls back to URI authority (HTTP/2).
    #[test]
    fn request_authority_host_falls_back_to_uri_when_host_header_missing() {
        assert_eq!(
            request_authority_host(None, Some("example.test")),
            "example.test"
        );
        assert_eq!(
            request_authority_host(Some(""), Some("www.example.test")),
            "www.example.test"
        );
        assert_eq!(
            request_authority_host(Some("example.test"), Some("evil.test")),
            "example.test"
        );
        assert_eq!(request_authority_host(None, None), "");
        assert!(should_serve_apex_coming_soon(
            &request_authority_host(None, Some("example.test")),
            "example.test",
            "services.example.test",
            "/"
        ));
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
