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
  # enableACME on these vhosts is what creates the nixpkgs ACME units.
  acmeCertNames = lib.unique [
    cfg.servicesHostname
    cfg.mailHostname
    cfg.primaryDomain
    "www.${cfg.primaryDomain}"
  ];
  acmeJournal = name: {
    serviceConfig = {
      SyslogIdentifier = name;
      LogRateLimitIntervalSec = "30s";
      LogRateLimitBurst = 50000;
    };
  };
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
        # Default error_log level is error. notice adds TLS and worker
        # notices without debug buffers (those can include certificate bytes).
        logError = "stderr notice";

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
            # No public /stalwart-admin/ proxy. That path published loopback
            # Stalwart HTTP on the services vhost. Bootstrap is an SSH tunnel
            # to the loopback listener, not a clearnet location.
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

      # Journal identity for the transitional edge and for the ACME units
      # this escape creates (acme-<cert>, acme-order-renew-<cert>). The
      # renew timer is acme-renew-<cert> and has no exec log of its own.
      # In-process host ACME stays off. web.enable stays the gate.
      systemd.services = lib.mkMerge [
        {
          nginx.serviceConfig = {
            SyslogIdentifier = "nginx";
            LogRateLimitIntervalSec = "30s";
            LogRateLimitBurst = 50000;
          };
        }
        (lib.listToAttrs (
          map (cert: {
            name = "acme-${cert}";
            value = acmeJournal "acme-${cert}";
          }) acmeCertNames
        ))
        (lib.listToAttrs (
          map (cert: {
            name = "acme-order-renew-${cert}";
            value = acmeJournal "acme-order-renew-${cert}";
          }) acmeCertNames
        ))
      ];
    }
  ]);
}
