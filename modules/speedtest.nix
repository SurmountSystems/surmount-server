# Ookla Speedtest CLI (wrapped). Default off except the mail host sets enable.
# Periodic JSON timer stays off until surmount.speedtest.periodic.enable.
# nix run .#speedtest
# Module: surmount.speedtest.enable
# Timer:  surmount.speedtest.periodic.enable (default every 6 hours)

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount.speedtest;
  inherit (lib)
    mkEnableOption
    mkIf
    mkMerge
    mkOption
    types
    optionals
    escapeShellArgs
    optionalString
    ;
  speedtestBin = "${cfg.package}/bin/speedtest";
  jsonArgs = [
    "-f"
    "json"
  ]
  ++ optionals (cfg.periodic.serverId != null) [
    "--server-id"
    (toString cfg.periodic.serverId)
  ]
  ++ cfg.periodic.extraFlags;
in
{
  options.surmount.speedtest = {
    enable = mkEnableOption "wrapped Ookla Speedtest CLI on PATH";

    package = mkOption {
      type = types.package;
      default =
        pkgs.speedtest or (throw "pkgs.speedtest missing (fail-closed); enable the flake overlay");
      defaultText = "pkgs.speedtest";
      description = "Wrapped speedtest package (license and GDPR flags already accepted).";
    };

    periodic = {
      enable = mkEnableOption "systemd timer that runs a JSON speedtest";

      interval = mkOption {
        type = types.str;
        default = "6h";
        description = "systemd OnUnitActiveSec for the timer (default every 6 hours).";
      };

      serverId = mkOption {
        type = types.nullOr types.ints.positive;
        default = null;
        description = "Optional Ookla server id passed as --server-id.";
      };

      extraFlags = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = "Extra flags after -f json (and --server-id when set).";
      };

      logFile = mkOption {
        type = types.nullOr types.str;
        default = null;
        description = ''
          If set, append each JSON result to this path as well as the journal.
          The parent directory must exist and be writable by root, and the
          path must not be under /home or /root (ProtectHome is on). When
          this is null the oneshot uses DynamicUser instead of root.
        '';
      };
    };
  };

  config = mkMerge [
    # ProtectHome is on for the oneshot. A log path under /home or /root
    # would be a false miss or a root write into a home directory.
    (mkIf (cfg.periodic.logFile != null) {
      assertions = [
        {
          assertion =
            let
              p = cfg.periodic.logFile;
            in
            lib.hasPrefix "/" p
            && p != "/home"
            && p != "/root"
            && !(lib.hasPrefix "/home/" p)
            && !(lib.hasPrefix "/root/" p)
            && !(lib.hasInfix ".." p);
          message = ''
            surmount.speedtest.periodic.logFile must be an absolute path
            and must not be under /home or /root.
          '';
        }
      ];
    })
    (mkIf cfg.enable {
      # Wrapper getName is speedtest; upstream attr is ookla-speedtest.
      nixpkgs.config.allowUnfreePredicate =
        pkg:
        builtins.elem (lib.getName pkg) [
          "ookla-speedtest"
          "speedtest"
        ];
      environment.systemPackages = [ cfg.package ];
    })
    (mkIf (cfg.enable && cfg.periodic.enable) {
      systemd.services.surmount-speedtest = {
        description = "Ookla Speedtest JSON probe";
        serviceConfig = {
          Type = "oneshot";
          # JSON on stdout is the probe result, not a secret. Keep it in
          # the journal under a stable identifier. Burst matches journald
          # so one large result is not the line that gets suppressed.
          SyslogIdentifier = "surmount-speedtest";
          StandardOutput = "journal";
          StandardError = "journal";
          LogRateLimitIntervalSec = "30s";
          LogRateLimitBurst = 50000;
          NoNewPrivileges = true;
          PrivateTmp = true;
          ProtectSystem = "strict";
          ProtectHome = true;
          ProtectKernelTunables = true;
          ProtectKernelModules = true;
          ProtectControlGroups = true;
          RestrictNamespaces = true;
          LockPersonality = true;
          MemoryDenyWriteExecute = true;
          RestrictRealtime = true;
          SystemCallArchitectures = "native";
          # Outbound probe plus resolved on the unix socket.
          RestrictAddressFamilies = [
            "AF_INET"
            "AF_INET6"
            "AF_UNIX"
          ];
        }
        // (if cfg.periodic.logFile == null then { DynamicUser = true; } else { User = "root"; });
        script = ''
          set -euo pipefail
          ${lib.escapeShellArg speedtestBin} ${escapeShellArgs jsonArgs} ${
            optionalString (cfg.periodic.logFile != null) "| tee -a ${lib.escapeShellArg cfg.periodic.logFile}"
          }
        '';
      };
      systemd.timers.surmount-speedtest = {
        description = "Periodic Ookla Speedtest JSON probe";
        wantedBy = [ "timers.target" ];
        timerConfig = {
          OnBootSec = "5min";
          OnUnitActiveSec = cfg.periodic.interval;
          Persistent = true;
          Unit = "surmount-speedtest.service";
        };
      };
    })
  ];
}
