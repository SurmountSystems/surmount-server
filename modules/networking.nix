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
  inherit (lib) mkIf mkDefault mkMerge;
in
{
  # mkMerge so the eth0 unit flags stay behind the interface gate.
  # Those flags are not set when eth0 is absent.
  config = mkIf cfg.enable (mkMerge [
  {
    networking.firewall = {
      enable = mkDefault true;
      # Refused connections (not every packet) go to the kernel journal.
      # logRefusedPackets stays off: that option is a packet debug flood.
      # rejectPackets stays false so a scan does not get an ICMP reply.
      logRefusedConnections = mkDefault true;
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

    systemd.services.firewall.serviceConfig = {
      SyslogIdentifier = "firewall";
      LogRateLimitIntervalSec = "30s";
      LogRateLimitBurst = 50000;
    };

    # A switch must not stop sshd. Stopping sshd.service drops SSH.
    # restartIfChanged false skips the unit. stopIfChanged false would
    # restart it, and that restart is not this stop list.
    systemd.services.sshd.stopIfChanged = false;
    systemd.services.sshd.restartIfChanged = false;

    # Host name stays in hosts/*/configuration.nix so NixOS VM tests
    # (nodes.<name>) can set their own without conflicts.
  }
  (mkIf (config.networking.interfaces ? eth0) {
    # A switch must not stop network-addresses-eth0. Stopping that
    # unit drops addresses. Same stopIfChanged and restartIfChanged
    # pair as sshd. The gate is the eth0 interface.
    systemd.services.network-addresses-eth0.stopIfChanged = false;
    systemd.services.network-addresses-eth0.restartIfChanged = false;
  })
  ]);
}
