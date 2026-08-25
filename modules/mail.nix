# Stalwart Mail Server wiring for Surmount (engine 0.16+).
#
# Module path: services.stalwart (Surmount-owned in modules/stalwart-service.nix;
# both stock paths dual-disabled there: stalwart-mail.nix + stalwart.nix).
# Option name matches stock nixpkgs 26.05; Surmount still owns 0.16 config.json
# (not the stock TOML module body). Unit remains stalwart-mail.service.
#
# Stalwart owns SMTP/IMAP/JMAP/ManageSieve (and collab protocols). We do not
# reimplement an MTA. Management UI and operator tooling live in the Rust crate
# + docs + stalwart-cli.
#
# P1 product edge (operator lock): public clearnet :80/:443 belong to
# management-ui (Axum rustls), not Stalwart. Stalwart first-boot may still
# insert HTTPS :443; free it with the apply plan under /etc/surmount/stalwart/
# (nix/stalwart/) before public B1. Mail ports stay on Stalwart.
# See docs/EDGE_AND_TLS.md, docs/OPS.md.
#
# 0.16 config model (read UPGRADING notes in the pinned tag):
#   - On disk: only config.json describing the DataStore (here: RocksDB path).
#   - Everything else (listeners, domains, accounts, spam, TLS, webui URL)
#     lives in the datastore as JMAP objects. First boot with an empty store
#     inserts upstream safe defaults (SMTP :25, submissions :465, IMAPS :993,
#     ManageSieve :4190, HTTP :8080, HTTPS :443, POP3S :995).
#   - Day-2 config: WebUI or `stalwart-cli apply` (declarative plan files).
#   - Docs: https://stalw.art/docs/  and tag UPGRADING/v0_16.md
#
# Store layout (Surmount provisional choice, not "because nixpkgs said so"):
#   docs/DATASTORES.md  - four-store model, RocksDB ops, backup consistency
#   One RocksDB at ${mailDataDir}/db for data+blob+fts+lookup roles.
# Override storePath / configFile only with a DATASTORES update the same turn.
#
# Spam-filter and WebUI FODs are packaged under pkgs.stalwart-mail.spam-filter
# and pkgs.stalwart-mail.webui. Upstream first-boot defaults may still try
# GitHub for webui.zip / spam rules until you point Application.resource_url
# and spam rules at file:// paths via apply. See .grok/joins/stalwart-current.md.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf mkMerge concatMapStringsSep;
  allDomains = [ cfg.primaryDomain ] ++ cfg.additionalDomains;

  spamFilter = config.services.stalwart.package.spam-filter or null;
  webui = config.services.stalwart.package.webui or null;
in
{
  config = mkIf cfg.enable (mkMerge [
    {
      services.stalwart = {
        enable = true;
        # Prefer Surmount networking.nix as firewall SoT (mail + Axum edge).
        # openFirewall never includes product :443 (mail-plane only if forced).
        openFirewall = lib.mkDefault false;
        dataDir = cfg.mailDataDir;
        storeType = "RocksDb";
        storePath = "${cfg.mailDataDir}/db";

        # Hostname for logs / defaults. Public URL can be set later.
        extraEnvironment = {
          STALWART_HOSTNAME = cfg.mailHostname;
        };

        # Recommended default: durable recovery EnvironmentFile (path only).
        # Unit uses EnvironmentFile=-path so a missing file after strip does
        # not fail start. Operator can set "" in host-local to opt out.
        # Password body never enters Nix. See docs/OPS.md.
        recoveryAdminEnvFile = lib.mkDefault "/var/lib/surmount/secrets/stalwart/recovery.env";

        # settings is accepted-and-ignored (0.16 has no TOML). Kept empty so
        # greps for server.listener in this repo do not pretend we still
        # declaratively pin binds here; defaults come from first-boot JMAP.
      };

      # ProtectSystem=strict only grants dataDir write. Dual-sign File PEMs
      # live under Domain B /var/lib/surmount/secrets/mail/dkim/. Mail-plane
      # TLS File PEMs are the same durable Let's Encrypt files Axum uses.
      # Missing path is ignored by systemd ReadOnlyPaths.
      systemd.services.stalwart-mail.serviceConfig.ReadOnlyPaths = [
        "/var/lib/surmount/secrets/mail/dkim"
        "/var/lib/surmount/secrets/tls"
      ];

      # Key is 0640 surmount-ui:surmount-tls (not world-readable). Stalwart
      # needs the group; Axum stays owner-read. Do not put PEM bodies in Nix.
      users.groups.surmount-tls = { };
      users.users.stalwart-mail.extraGroups = [ "surmount-tls" ];
      systemd.services.stalwart-mail.serviceConfig.SupplementaryGroups = [ "surmount-tls" ];
      systemd.services.surmount-management-ui.serviceConfig.SupplementaryGroups =
        lib.mkIf cfg.managementUi.enable
          [ "surmount-tls" ];

      # Binary + CLI + official 0.16 Maildir importer (Vandelay) + wrapper.
      environment.systemPackages = [
        config.services.stalwart.package
        pkgs.stalwart-cli
        pkgs.vandelay
        pkgs.surmount-mail-import
        # Post-switch smoke + deploy bins on PATH after this generation.
        pkgs.surmount-deploy-host
      ];

      # Drop hermetic resource paths for operators (not auto-consumed by 0.16
      # until pointed at via apply / WebUI). P1 free-:443 apply plan is
      # operator-run after first boot (not auto-applied; needs admin token).
      environment.etc = lib.mkMerge [
        {
          "surmount/mail-domains.txt".text = concatMapStringsSep "\n" (d: d) allDomains + "\n";
          "surmount/stalwart/free-public-443-for-axum-edge.ndjson".source =
            ../nix/stalwart/free-public-443-for-axum-edge.ndjson;
          "surmount/stalwart/README-free-public-443.txt".source = ../nix/stalwart/README-free-public-443.txt;
          # DKIM Day-1: Ed25519 example apply plan + host runbook (operator
          # applies after valid stalwart-token). Private key is Domain B path,
          # never in git.
          "surmount/stalwart/dkim-signature-ed25519-stalwart.example.ndjson".source =
            ../nix/stalwart/dkim-signature-ed25519-stalwart.example.ndjson;
          "surmount/stalwart/dkim-signature-rsa-stalwart.example.ndjson".source =
            ../nix/stalwart/dkim-signature-rsa-stalwart.example.ndjson;
          "surmount/stalwart/README-dkim-surmount-systems.txt".source =
            ../nix/stalwart/README-dkim-surmount-systems.txt;
          "surmount/stalwart/mail-plane-tls-le-pems.example.ndjson".source =
            ../nix/stalwart/mail-plane-tls-le-pems.example.ndjson;
          "surmount/stalwart/README-mail-plane-tls.txt".source = ../nix/stalwart/README-mail-plane-tls.txt;
          # Inline text so flake eval does not depend on a new untracked source
          # file. Default is already Argon2id; this is a host-visible pin.
          "surmount/stalwart/README-mailbox-password-argon2id.txt".text = ''
            Mailbox password hashing (Stalwart 0.16.15)

            The services console POST /api/v1/accounts/password sends the plaintext
            password over the authenticated operator session to loopback Stalwart.
            The engine hashes it. Surmount does not pre-hash in the browser or Axum.

            Stalwart Authentication.passwordHashAlgorithm default is argon2id.
            That is the 0.16.15 source default (PasswordHashAlgorithm::Argon2id)
            and the public docs default.

            Existing empty mailbox credentials stay empty until you set a password
            on /mail. Changing the algorithm does not rewrite stored hashes.

            Verify (on the host, with a Stalwart-accepted token):

              stalwart-cli get Authentication

            Optional pin (only if the field is not already argon2id):

              stalwart-cli update Authentication --field passwordHashAlgorithm=argon2id

            Docs: https://stalw.art/docs/auth/authentication/password/
                  https://stalw.art/docs/ref/object/authentication/#passwordhashalgorithm
                  (accessed: 2026-08-14)
                  docs/SECURITY.md (Mailbox password hashing)
          '';
          "surmount/stalwart/README-imap-timeout-anonymous.txt".text = ''
            IMAP unauthenticated timeout (Stalwart Imap singleton)

            Stalwart ends an IMAP session that is still NotAuthenticated when
            the next client read waits longer than timeoutAnonymous. The server
            writes: * BYE Connection timed out.

            Evolution shows that BYE as Failed to authenticate even when the
            mailbox password is already set. A first open of a large imported
            mailbox (cert exception, extra IMAP connections, folder scan) can
            sit longer than the engine default of 60000 ms (one minute).

            Product pin for this host: timeoutAnonymous = 1800000 (30 minutes),
            matching timeoutAuthenticated. Datastore field; survives deploy.
            First-boot empty store still starts at 60000 until this is applied.

            Verify (on the host, with a Stalwart-accepted token):

              stalwart-cli get Imap --fields timeoutAnonymous,timeoutAuthenticated --json

            Optional pin (only if timeoutAnonymous is still 60000):

              stalwart-cli update Imap --field timeoutAnonymous=1800000

            Docs: https://stalw.art/docs/email/settings/imap/
                  https://stalw.art/docs/ref/object/imap#timeoutanonymous
                  (accessed: 2026-08-14)
          '';
        }
        (lib.mkIf (spamFilter != null) {
          "surmount/stalwart/spam-filter.toml".source = "${spamFilter}/spam-filter.toml";
          "surmount/stalwart/spam-filter-rules.json.gz".source = "${spamFilter}/spam-filter-rules.json.gz";
        })
        (lib.mkIf (webui != null) {
          "surmount/stalwart/webui.zip".source = "${webui}/webui.zip";
        })
      ];

      # Staging area for operator-copied Maildir trees (not auto-imported).
      # z heals existing durable TLS leaves after deploy even if install ran
      # before group surmount-tls existed (0640, not world-readable).
      systemd.tmpfiles.rules = [
        "d ${cfg.stateDir} 0750 root root - -"
        "d ${cfg.stateDir}/import 0750 root root - -"
        "d ${cfg.stateDir}/import/maildir 0700 root root - -"
        "d ${cfg.stateDir}/import/vandelay 0700 root root - -"
        "z ${cfg.secrets.durableMaterialDir}/tls/cert.pem 0640 surmount-ui surmount-tls -"
        "z ${cfg.secrets.durableMaterialDir}/tls/key.pem 0640 surmount-ui surmount-tls -"
      ];
    }

    {
      system.activationScripts.surmount-mail-hint.text = ''
        echo "surmount: mail domains -> /etc/surmount/mail-domains.txt"
        echo "surmount: import helper -> surmount-mail-import-maildir"
        echo "surmount: Stalwart 0.16 config.json (DataStore only); management on :8080 after first-boot defaults"
        echo "surmount: hermetic FODs (wire via apply/WebUI) -> /etc/surmount/stalwart/"
        echo "surmount: P1 free public :443 for Axum -> /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson"
        echo "surmount: product clearnet HTTPS is management-ui, not Stalwart :443"
      '';
    }
  ]);
}
