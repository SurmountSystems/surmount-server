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
        # CFS, not SCHED_FIFO. Nice -20 is the last line. Do not set
        # CPUSchedulingPolicy to fifo or rr, and do not set realtime I/O.
        Nice = -20;
        OOMScoreAdjust = -1000;
        # Watchdog must be able to kill other uids. Do not set PrivateUsers,
        # ProcSubset, or ProtectProc: those hide other processes or block
        # cross-uid SIGKILL. /proc/meminfo and systemctl kill stay available.
        User = "root";
        NoNewPrivileges = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectHostname = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        RestrictNamespaces = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        # systemctl kill talks to the private socket. libc kill needs no socket.
        RestrictAddressFamilies = [ "AF_UNIX" ];
        UMask = "0077";
        # stderr plan and kill lines (no secret values). Default per-unit
        # burst is 10000/30s; match the journal cap so a flood does not
        # drop the first kill line.
        SyslogIdentifier = "surmount-scram";
        StandardOutput = "journal";
        StandardError = "journal";
        LogRateLimitIntervalSec = "30s";
        LogRateLimitBurst = 50000;
      };
    };

    boot.kernel.sysctl."vm.swappiness" = mkDefault 1;
  };
}
