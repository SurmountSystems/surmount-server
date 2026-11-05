# Security defaults: SSH hardening, sshguard, edge-friendly limits.
# nginx is transitional-to-delete; prefer limits that transfer to Axum-first edge.
# Keep this boring and reversible; deep custom hardening belongs in host config.
#
# Merciless access control (operator direction 2026-07-30):
# - IP rate limits at Axum edge (management-ui FixedWindowRateLimiter)
# - Ban decision layer in management-ui (default enforcement off; lean private)
# - Unauthorized access -> fast/immediate host blacklist (nftables lean; Q-ACL-1 open)
# - Explicit whitelist IPs never banned; track last-used for hygiene
# - Long-term prefer Rust ban helper over a host Python service
# sshguard is the sshd ban: permanent after 3 failures. fail2ban is not
# installed. Design notes:
#   docs/research/access-control-fail2ban.md
# Do not add mail watchers here. Stalwart owns mail anti-abuse first.
#
# SSH host keys: advertise and generate ed25519 only. NixOS 26.05 default
# also offers RSA; this module stops that. Existing RSA files on disk are
# not deleted by dropping rsa from hostKeys. Classical SSH hygiene only:
# Ed25519 is not post-quantum; OpenSSH host keys are not PQ; PQConnect is
# not a host-key swap.
#
# When surmount.accessControl.enable: named nft sets are installed (empty).
# That is not "live bans work" without operator nft load + enforcement wiring.
# Set names match crates/management-ui/src/ban.rs constants.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  ac = cfg.accessControl;
  inherit (lib) mkIf mkDefault mkMerge;

  # Canonical set/table names (must match Rust ban.rs NFT_* constants).
  nftTable = "surmount_guard";
  nftBan4 = "surmount-ban4";
  nftBan6 = "surmount-ban6";
  nftWl4 = "surmount-whitelist4";
  nftWl6 = "surmount-whitelist6";
in
{
  config = mkMerge [
    (mkIf (cfg.enable && cfg.hardening.enable) {
      services.openssh = {
        enable = true;
        hostKeys = [
          {
            type = "ed25519";
            path = "/etc/ssh/ssh_host_ed25519_key";
          }
        ];
        settings = {
          PasswordAuthentication = cfg.hardening.allowPasswordAuth;
          KbdInteractiveAuthentication = false;
          PermitRootLogin = mkDefault "prohibit-password";
          X11Forwarding = false;
          AllowAgentForwarding = mkDefault false;
          # Empty passwords and user ~/.ssh/environment are not an option.
          # When password auth is off, publickey is the only method so a
          # later default cannot turn keyboard-interactive back on.
          PermitEmptyPasswords = false;
          PermitUserEnvironment = false;
          # Default grace is 120 seconds of unauthenticated hold.
          LoginGraceTime = 60;
        }
        // lib.optionalAttrs (!cfg.hardening.allowPasswordAuth) {
          AuthenticationMethods = "publickey";
        };
      };

      # sshguard 2.5.1 (pinned nixos-26.05): an sshd auth failure scores
      # 10. Block and blacklist both fire when cumulated danger >= the
      # threshold, so 30 is 3 failures. blacklist pardontime 0 is never
      # unblocked and is reloaded from blacklist_file. The nixpkgs module
      # uses sshg-fw-ipset with the host iptables firewall, or
      # sshg-fw-nft-sets when networking.nftables.enable is true, and
      # partOf firewall.service. fail2ban is Python and stays off.
      # Do not enable surmount.accessControl or the nft helper.
      services.fail2ban.enable = false;
      services.sshguard = {
        enable = true;
        services = [ "sshd" ];
        attack_threshold = 30;
        blacklist_threshold = 30;
        blacklist_file = "/var/lib/sshguard/blacklist.db";
      };

      systemd.services.sshguard.serviceConfig = {
        SyslogIdentifier = "sshguard";
        LogRateLimitIntervalSec = "30s";
        LogRateLimitBurst = 50000;
      };

      # Kernel / network hygiene (small set; avoid surprising operators).
      boot.kernelParams = mkDefault [ "page_alloc.shuffle=1" ];

      security.sudo.execWheelOnly = mkDefault true;
    })

    # SHC 261: guest agent so the hypervisor can inject a command when
    # SSH is stuck. Default off; host-local enables on this QEMU VPS.
    (mkIf (cfg.enable && cfg.hardening.enable && cfg.hardening.qemuGuestAgent.enable) {
      services.qemuGuest.enable = true;
      # Stock nixpkgs only starts the unit via udev SYSTEMD_WANTS on the
      # virtio port. After a switch that udev event has already fired, so
      # the unit stays dead. Want multi-user so SHC inject works on boot
      # and after this generation.
      systemd.services.qemu-guest-agent.wantedBy = [ "multi-user.target" ];
      # Start and guest-exec lines stay in the journal. The option stays
      # default off; this does not enable the agent.
      systemd.services.qemu-guest-agent.serviceConfig = {
        SyslogIdentifier = "qemu-guest-agent";
        LogRateLimitIntervalSec = "30s";
        LogRateLimitBurst = 50000;
      };
    })

    # Access-control nft fragment: opt-in, lean private default off.
    (mkIf (cfg.enable && ac.enable && ac.nftSets) {
      networking.nftables.enable = true;
      systemd.services.nftables.serviceConfig = {
        SyslogIdentifier = "nftables";
        LogRateLimitIntervalSec = "30s";
        LogRateLimitBurst = 50000;
      };
      networking.nftables.tables.${nftTable} = {
        family = "inet";
        content = ''
          set ${nftBan4} {
            type ipv4_addr
            flags timeout
            comment "Surmount merciless ban v4"
          }
          set ${nftBan6} {
            type ipv6_addr
            flags timeout
            comment "Surmount merciless ban v6"
          }
          set ${nftWl4} {
            type ipv4_addr
            flags interval
            comment "Surmount whitelist v4"
          }
          set ${nftWl6} {
            type ipv6_addr
            flags interval
            comment "Surmount whitelist v6"
          }
          chain input {
            type filter hook input priority -10; policy accept;
            ip saddr @${nftWl4} accept
            ip6 saddr @${nftWl6} accept
            ${
              if (cfg.logging.enable or false) then
                ''
                  ip saddr @${nftBan4} log prefix "surmount-nft-ban-drop: " level info drop
                  ip6 saddr @${nftBan6} log prefix "surmount-nft-ban-drop: " level info drop
                ''
              else
                ''
                  ip saddr @${nftBan4} drop
                  ip6 saddr @${nftBan6} drop
                ''
            }
          }
        '';
      };
    })

    # Fail-closed Nix assertions for accessControl knobs (no invented Q-ACL).
    (mkIf (cfg.enable && ac.enable) {
      assertions = [
        {
          assertion = !(ac.nftExec && ac.nftHelper);
          message = ''
            surmount.accessControl.nftExec and nftHelper are mutually exclusive.
            Prefer nftHelper (least-privilege CAP_NET_ADMIN on the helper only).
          '';
        }
        {
          # Host apply only runs when backend=nft (BanGuard Memory branch ignores
          # helper sock). Fail-closed so nftHelper=true is never a silent no-op.
          assertion = ac.nftHelper -> (ac.backend == "nft");
          message = ''
            surmount.accessControl.nftHelper is true but backend is not "nft".
            Set accessControl.backend = "nft" (and enforcement = "enforce" for
            live host drop). Memory backend never calls the helper; the privileged
            socket would be installed with no apply path.
          '';
        }
        {
          assertion = ac.nftExec -> (ac.backend == "nft");
          message = ''
            surmount.accessControl.nftExec is true but backend is not "nft".
            Set accessControl.backend = "nft", or set nftExec = false.
          '';
        }
        {
          assertion = ac.nftExec -> (ac.nftBin != "" && lib.hasPrefix "/" ac.nftBin);
          message = ''
            surmount.accessControl.nftExec is true but nftBin is empty or not an
            absolute path. Point nftBin at the host nft binary, or set nftExec =
            false (default; prefer nftHelper).
          '';
        }
        {
          assertion = ac.nftHelper -> (ac.nftBin != "" && lib.hasPrefix "/" ac.nftBin);
          message = ''
            surmount.accessControl.nftHelper is true but nftBin is empty or not
            an absolute path. The helper needs SURMOUNT_BAN_NFT_BIN to run nft.
          '';
        }
        {
          assertion =
            ac.nftHelperBin == ""
            || (lib.hasPrefix "/" ac.nftHelperBin && !(lib.hasInfix ".." ac.nftHelperBin));
          message = ''
            surmount.accessControl.nftHelperBin must be empty or an absolute host
            path without "..".
          '';
        }
        {
          assertion =
            ac.statePath == "" || (lib.hasPrefix "/" ac.statePath && !(lib.hasInfix ".." ac.statePath));
          message = ''
            surmount.accessControl.statePath must be empty or an absolute host
            path without "..".
          '';
        }
      ];
      warnings = lib.optional ac.nftExec ''
        surmount.accessControl.nftExec is true: management-ui does NOT get
        CAP_NET_ADMIN (NoNewPrivileges stays on). In-process nft add-element
        will fail at runtime. Prefer nftHelper = true (socket-activated oneshot
        with CAP_NET_ADMIN on the helper unit) or app-level enforcement only.
        See docs/EDGE_AND_TLS.md operator smoke.
      '';
    })

    # Least-privilege elevation: socket-activated oneshot helper unit.
    # CAP_NET_ADMIN lives on the helper service only. UI keeps NNP and never
    # spawns a setcap child (PR_SET_NO_NEW_PRIVS would block file caps anyway).
    # Accept=yes oneshot avoids a long-running daemon restart-loop class.
    (mkIf (cfg.enable && ac.enable && ac.nftHelper) (
      let
        uiPkg =
          if cfg.managementUi.package != null then cfg.managementUi.package else pkgs.surmount-management-ui;
        helperBin =
          if ac.nftHelperBin != "" then ac.nftHelperBin else "${uiPkg}/bin/surmount-nft-ban-helper";
        helperSock = "/run/surmount/nft-ban-helper.sock";
      in
      {
        assertions = [
          {
            assertion = ac.nftHelperBin != "" || uiPkg != null;
            message = ''
              surmount.accessControl.nftHelper is true but management-ui package
              is null and nftHelperBin is empty. Set managementUi.package or
              nftHelperBin to an absolute helper binary path.
            '';
          }
        ];

        # Ensure UI user exists when helper is on without managementUi.enable
        # (socket group). management-ui module also creates the user when UI on.
        users.users.surmount-ui = {
          isSystemUser = true;
          group = "surmount-ui";
          description = "Surmount management UI";
        };
        users.groups.surmount-ui = { };

        systemd.sockets.surmount-nft-ban-helper = {
          description = "Surmount nft ban helper socket (Accept=yes)";
          wantedBy = [ "sockets.target" ];
          socketConfig = {
            ListenStream = helperSock;
            Accept = true;
            # UI connects; helper unit is root + AmbientCapabilities.
            SocketUser = "root";
            SocketGroup = "surmount-ui";
            SocketMode = "0660";
            # Directory for the sock under /run/surmount.
            RuntimeDirectory = "surmount";
            RuntimeDirectoryMode = "0755";
          };
        };

        systemd.services."surmount-nft-ban-helper@" = {
          description = "Surmount nft ban helper oneshot (per connection)";
          # No Restart: oneshot; fail-closed connection does not storm.
          serviceConfig = {
            Type = "oneshot";
            ExecStart = "${helperBin} apply-systemd-socket";
            Environment = [
              "SURMOUNT_BAN_NFT_BIN=${ac.nftBin}"
            ];
            # Privileges only here — not on management-ui.
            AmbientCapabilities = [
              "CAP_NET_ADMIN"
              "CAP_NET_RAW"
            ];
            CapabilityBoundingSet = [
              "CAP_NET_ADMIN"
              "CAP_NET_RAW"
            ];
            NoNewPrivileges = true;
            PrivateTmp = true;
            ProtectSystem = "strict";
            ProtectHome = true;
            ProtectKernelTunables = true;
            ProtectKernelModules = true;
            ProtectControlGroups = true;
            # nft uses netlink; UDS is the client channel.
            RestrictAddressFamilies = [
              "AF_UNIX"
              "AF_NETLINK"
            ];
            RestrictNamespaces = true;
            LockPersonality = true;
            MemoryDenyWriteExecute = true;
            RestrictRealtime = true;
            SystemCallArchitectures = "native";
            # StandardInput inherits the connected socket from Accept=yes.
            StandardInput = "socket";
            StandardOutput = "socket";
            StandardError = "journal";
            # stderr is the helper log. stdout stays the socket (the
            # protocol). Do not log nft payloads or set contents.
            SyslogIdentifier = "surmount-nft-ban-helper";
            LogRateLimitIntervalSec = "30s";
            LogRateLimitBurst = 50000;
          };
        };
      }
    ))
  ];
}
