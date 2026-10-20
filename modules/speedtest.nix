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
          The parent directory must exist and be writable by root.
        '';
      };
    };
  };

  config = mkMerge [
    (mkIf cfg.enable {
      nixpkgs.config.allowUnfreePredicate = pkg: lib.getName pkg == "ookla-speedtest";
      environment.systemPackages = [ cfg.package ];
    })
    (mkIf (cfg.enable && cfg.periodic.enable) {
      systemd.services.surmount-speedtest = {
        description = "Ookla Speedtest JSON probe";
        serviceConfig = {
          Type = "oneshot";
          User = "root";
        };
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
