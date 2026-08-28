# Highest-priority last line: surmount-scram --watch.
# Same binary as `just scram` (laptop SSH --now) and guest `just scram` (local --now).

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf mkDefault;
  scram = "${pkgs.surmount-scram}/bin/surmount-scram";
in
{
  config = mkIf cfg.enable {
    environment.systemPackages = [ pkgs.surmount-scram ];

    systemd.services.surmount-scram = {
      description = "Surmount scram: SIGKILL builder hogs when RAM is nearly gone";
      wantedBy = [ "multi-user.target" ];
      after = [ "local-fs.target" ];
      serviceConfig = {
        Type = "simple";
        ExecStart = "${scram} --watch";
        Restart = "always";
        RestartSec = "1s";
        Nice = -20;
        CPUSchedulingPolicy = "fifo";
        CPUSchedulingPriority = 99;
        IOSchedulingClass = "realtime";
        IOSchedulingPriority = 0;
        OOMScoreAdjust = -1000;
        # Watchdog must be able to kill other uids.
        User = "root";
      };
    };

    boot.kernel.sysctl."vm.swappiness" = mkDefault 1;
  };
}
