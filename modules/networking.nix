# Firewall and basic networking for the mail VPS.
# Opens only the ports the stack actually needs.
#
# Access-control lean (with modules/hardening.nix):
# - Public ports limited to mail + clearnet edge (:80/:443) + SSH
# - Ban/whitelist nft sets are host opt-in (see hardening header)
# - Do not expose Stalwart :8080 or management UI loopback ports publicly
# - Arti HS does not need extra public TCP for onion publish (Tor circuits)

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf mkDefault;
in
{
  config = mkIf cfg.enable {
    networking.firewall = {
      enable = mkDefault true;
      allowedTCPPorts = [
        22 # SSH (consider restrict-to-admin nets later)
        25 # SMTP inbound
        80 # HTTP (product redirect-only / dual-run ACME; not Stalwart)
        443 # HTTPS product edge (Axum management-ui; nginx dual-run escape only)
        465 # SMTPS submission
        587 # SMTP submission (STARTTLS)
        993 # IMAPS
        4190 # ManageSieve
      ];
      # P1: :80/:443 are product clearnet edge (Axum when web.enable false).
      # Stalwart is not the product public HTTPS owner. First-boot may still
      # insert engine HTTPS :443 until operator apply plan
      # /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson (or WebUI).
      # Do not expose Stalwart HTTP (:8080 default) or UI loopback ports
      # publicly without intent. Rebind HTTP management to 127.0.0.1:8080.
      # Dual-run nginx only if surmount.web.enable = true.
      # See docs/EDGE_AND_TLS.md, nix/stalwart/README.md.
      #
      # Merciless ban: when accessControl.enable+nftSets, inet surmount_guard
      # holds surmount-ban4/6 + whitelist sets (hardening.nix). App-level bans
      # live in management-ui; live nft element add still operator/helper residual.
      # Do not claim host drop works from unit env alone.
    };

    # Host name stays in hosts/*/configuration.nix so NixOS VM tests
    # (nodes.<name>) can set their own without conflicts.
  };
}
