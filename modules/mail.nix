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
        openFirewall = false;
        dataDir = cfg.mailDataDir;
        storeType = "RocksDb";
        storePath = "${cfg.mailDataDir}/db";

        # Hostname for logs / defaults. Public URL can be set later.
        extraEnvironment = {
          STALWART_HOSTNAME = cfg.mailHostname;
        };

        # settings is accepted-and-ignored (0.16 has no TOML). Kept empty so
        # greps for server.listener in this repo do not pretend we still
        # declaratively pin binds here; defaults come from first-boot JMAP.
      };

      # Binary + CLI + import helper on PATH.
      environment.systemPackages = [
        config.services.stalwart.package
        pkgs.stalwart-cli
        (pkgs.writeShellApplication {
          name = "surmount-mail-import-maildir";
          runtimeInputs = [
            pkgs.stalwart-cli
            pkgs.coreutils
          ];
          text = ''
            # One-shot helper template for MailPlus -> Stalwart Maildir import.
            # Does NOT run automatically. Operator-driven.
            #
            # Usage:
            #   surmount-mail-import-maildir <account@domain> <path-to-Maildir>
            #
            # Stalwart 0.16+ uses schema-driven stalwart-cli over JMAP. Confirm
            # subcommands with: stalwart-cli --help
            # See docs/MIGRATION.md (may lag the CLI rename; trust --help).

            set -euo pipefail

            if [[ $# -lt 2 ]]; then
              echo "Usage: $0 <account@domain> <path-to-Maildir>" >&2
              exit 2
            fi

            ACCOUNT="$1"
            MAILDIR="$2"

            if [[ ! -d "$MAILDIR" ]]; then
              echo "Maildir path not found: $MAILDIR" >&2
              exit 1
            fi

            # Default HTTP management listener after first-boot defaults.
            STALWART_URL="''${STALWART_URL:-http://127.0.0.1:8080}"

            echo "Importing nested Maildir for $ACCOUNT from $MAILDIR"
            echo "Target: $STALWART_URL"
            echo "Review docs/MIGRATION.md and stalwart-cli --help before production imports."

            exec stalwart-cli --url "$STALWART_URL" import messages \
              --format maildir-nested \
              "$ACCOUNT" \
              "$MAILDIR"
          '';
        })
      ];

      # Drop hermetic resource paths for operators (not auto-consumed by 0.16
      # until pointed at via apply / WebUI).
      environment.etc = lib.mkMerge [
        {
          "surmount/mail-domains.txt".text = concatMapStringsSep "\n" (d: d) allDomains + "\n";
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
      systemd.tmpfiles.rules = [
        "d ${cfg.stateDir} 0750 root root - -"
        "d ${cfg.stateDir}/import 0750 root root - -"
        "d ${cfg.stateDir}/import/maildir 0700 root root - -"
      ];
    }

    {
      system.activationScripts.surmount-mail-hint.text = ''
        echo "surmount: mail domains -> /etc/surmount/mail-domains.txt"
        echo "surmount: import helper -> surmount-mail-import-maildir"
        echo "surmount: Stalwart 0.16 config.json (DataStore only); management on :8080 after first-boot defaults"
        echo "surmount: hermetic FODs (wire via apply/WebUI) -> /etc/surmount/stalwart/"
      '';
    }
  ]);
}
