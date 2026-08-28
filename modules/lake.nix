# Optional Lean/Lake compute. Default off.
# Same cgroup class as the ssh-ng builder: Nice=19, idle ionice, hard
# MemoryMax on the service that runs lake (not only a user slice),
# CPUQuota auto = 95 percent of online CPUs when onlineCpus is set,
# never 95 percent of one CPU. Job slots stay under MemoryMax.
# Mail, management-ui, sshd, Arti, and networking stay un-niced.
# Do not start this unit on the live guest from an agent.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  lake = cfg.lake;
  inherit (lib) mkIf mkMerge;
  sliceName = "surmount-lake";
  cpuQuotaAuto = lake.cpuQuota == "auto" || lake.cpuQuota == "";
  sliceCpu =
    if !cpuQuotaAuto then
      { CPUQuota = lake.cpuQuota; }
    else if lake.onlineCpus != null then
      { CPUQuota = "${toString (95 * lake.onlineCpus)}%"; }
    else
      { };
in
{
  config = mkMerge [
    (mkIf cfg.enable {
      assertions = [
        {
          assertion = lake.enable || !(config.systemd.services ? surmount-lake);
          message = "surmount-lake is only allowed when surmount.lake.enable is true (default off, niced, MemoryMax). Do not overlay a Lake unit that starts without that option.";
        }
      ];
    })
    (mkIf (cfg.enable && lake.enable) {
      assertions = [
        {
          assertion = lake.package != null;
          message = "surmount.lake.package must be set when surmount.lake.enable is true (bin/lake). Default-off leaves package null so eval does not pull Lean.";
        }
        {
          assertion = lake.memoryMax != null && lake.memoryMax != "";
          message = "surmount.lake.memoryMax must be a non-empty systemd MemoryMax string when enable is true (empty/null refuse; scaffold default 4G is a Lake budget, not 95 percent of the whole guest; never publish the guest size in git).";
        }
        {
          assertion = builtins.substring 0 1 lake.workDir == "/";
          message = "surmount.lake.workDir must be an absolute path.";
        }
      ];

      users.groups.surmount-lake = { };
      users.users.surmount-lake = {
        isSystemUser = true;
        group = "surmount-lake";
        description = "Niced Lake (keyless; not ssh-ng)";
        home = lake.workDir;
      };

      systemd.tmpfiles.rules = [
        "d ${lake.workDir} 0750 surmount-lake surmount-lake -"
      ];

      systemd.slices.${sliceName} = {
        description = "Surmount Lake (hard memory cap)";
        sliceConfig = {
          MemoryAccounting = true;
          CPUAccounting = true;
          IOAccounting = true;
          MemoryMax = lake.memoryMax;
        }
        // sliceCpu;
      };

      systemd.services.surmount-lake = {
        description = "Niced memory-capped Lake (Lean). Default-off; enable from host-local only.";
        wantedBy = [ "multi-user.target" ];
        after = [ "network-online.target" ];
        serviceConfig = {
          Type = "simple";
          Restart = "no";
          TimeoutStartSec = "infinity";
          User = "surmount-lake";
          Group = "surmount-lake";
          WorkingDirectory = lake.workDir;
          ExecStart =
            if lake.package == null then
              "${pkgs.coreutils}/bin/false"
            else
              lib.escapeShellArgs (
                [
                  "${lake.package}/bin/lake"
                  "-j${toString lake.jobs}"
                ]
                ++ lake.extraArgs
              );
          Slice = "${sliceName}.slice";
          MemoryAccounting = true;
          CPUAccounting = true;
          IOAccounting = true;
          MemoryMax = lake.memoryMax;
          Nice = 19;
          IOSchedulingClass = "idle";
          OOMScoreAdjust = 500;
          SyslogIdentifier = "surmount-lake";
          StandardOutput = "journal";
          StandardError = "journal";
          LogRateLimitIntervalSec = "30s";
          LogRateLimitBurst = 50000;
          NoNewPrivileges = true;
        }
        // sliceCpu;
      };

      # Do not Nice= mail, management-ui, sshd, Arti, or networking here.
      systemd.services.stalwart-mail.serviceConfig.OOMScoreAdjust = lib.mkDefault (-300);
      systemd.services.surmount-management-ui.serviceConfig.OOMScoreAdjust = lib.mkDefault (-300);
      systemd.services.sshd.serviceConfig.OOMScoreAdjust = lib.mkDefault (-300);
      systemd.services.surmount-arti-hidden-service.serviceConfig.OOMScoreAdjust = lib.mkDefault (-300);
    })
  ];
}
