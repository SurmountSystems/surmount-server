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
        80 # HTTP (ACME + redirect / Axum upgrade)
        443 # HTTPS (nginx transitional or Axum edge)
        465 # SMTPS submission
        587 # SMTP submission (STARTTLS)
        993 # IMAPS
        4190 # ManageSieve
      ];
      # Do not expose Stalwart HTTP (:8080 default) or UI loopback ports
      # publicly without intent. 0.16 first-boot defaults bind HTTP on
      # [::]:8080; rebind to loopback or UDS for production. Public HTTPS:
      # management-ui rustls when web.enable is false (default); dual-run
      # nginx only if surmount.web.enable = true. See docs/EDGE_AND_TLS.md.
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
