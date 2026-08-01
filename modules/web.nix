# HTTPS edge: virtual hosts + ACME (Let's Encrypt) for Surmount.
# Reverse-proxies the management UI and optionally Stalwart admin/JMAP.
#
# TRANSITIONAL-TO-DELETE: nginx dual-run escape only.
# Product default: surmount.web.enable = false; management-ui rustls owns
# public HTTPS (listenMode=https + host PEMs). Set web.enable = true only
# while migrating. Operator direction 2026-07-30: NO nginx as product edge.
# Prefer first-party Axum HTTPS edge and UDS to local backends. Cert path is
# not locked ACME-only. Do not expand nginx features.
# See docs/EDGE_AND_TLS.md, docs/operator-direction.md, RESIDUAL.md.
#
# Legacy / migrated sites: STATIC FILES ONLY, no exceptions. Old Synology
# content had no app servers. Prefer root + try_files (or edge static later);
# do not add PHP/Node stacks for legacy vhosts.
# TODO(static-sites): optional helper for surmount.web.staticSites (name ->
# document root) once content is staged; until then use extraVhosts.

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf mkMerge mkDefault;
  uiUpstream = "${cfg.managementUi.listenAddress}:${toString cfg.managementUi.port}";
in
{
  config = mkIf (cfg.enable && cfg.web.enable) (mkMerge [
    {
      security.acme = {
        acceptTerms = true;
        defaults.email = cfg.acmeEmail;
      };

      services.nginx = {
        enable = true;
        recommendedGzipSettings = true;
        recommendedOptimisation = true;
        recommendedProxySettings = true;
        recommendedTlsSettings = true;

        # Mild request-size guard; mail blobs go through Stalwart, not nginx.
        clientMaxBodySize = mkDefault "25m";

        virtualHosts = {
          # Primary management UI
          "${cfg.servicesHostname}" = {
            forceSSL = true;
            enableACME = true;
            locations."/" = {
              proxyPass = "http://${uiUpstream}";
              proxyWebsockets = true;
              extraConfig = ''
                proxy_read_timeout 120s;
              '';
            };
            # Optional path to Stalwart native admin (temporary fallback).
            # Stalwart 0.16 first-boot default HTTP listener is :8080 (all
            # interfaces). Prefer SSH tunnel and/or rebind to loopback via
            # WebUI / stalwart-cli apply before exposing this path publicly.
            locations."/stalwart-admin/" = {
              proxyPass = "http://127.0.0.1:8080/";
              extraConfig = ''
                # Bootstrap only. Consider removing once management-ui covers ops.
                proxy_set_header Host $host;
              '';
            };
          };

          # Mail hostname: ACME cert useful for Stalwart TLS file paths later.
          "${cfg.mailHostname}" = {
            forceSSL = true;
            enableACME = true;
            # Placeholder landing; real client traffic is SMTP/IMAP.
            locations."/".return = "302 https://${cfg.servicesHostname}/";
          };

          # Apex: simple park / redirect until static legacy content is staged.
          # Legacy sites = static files only (docs/open-choices.md).
          "${cfg.primaryDomain}" = {
            forceSSL = true;
            enableACME = true;
            locations."/".return = "302 https://${cfg.servicesHostname}/";
          };

          # www alias
          "www.${cfg.primaryDomain}" = {
            forceSSL = true;
            enableACME = true;
            globalRedirect = cfg.primaryDomain;
          };
        }
        // cfg.web.extraVhosts;
      };
    }
  ]);
}
