# Operator paper trail: persistent, size-capped journald plus sshd VERBOSE
# and systemd-journal membership for journal readers (nixbuilder when the
# remote builder is on; surmount-lake when Lake is on). Completeness and
# start-of-OOM / cgroup-kill clues beat aggressive rate-limit vacuum.
# Size cap still exists so the disk cannot fill. Do not shrink
# nix-daemon / builder / Lake journals. Mail / management-ui / sshd /
# Arti stay un-niced. Never log secrets. Scaffold SystemMaxUse /
# RuntimeMaxUse / RateLimitBurst are not guest SKUs.
#
# Living: docs/OPS.md (Logging), docs/SECURITY.md, docs/DATASTORES.md.

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount;
  log = cfg.logging;
  rb = cfg.remoteBuilder;
  lake = cfg.lake;
  inherit (lib) mkIf mkMerge;
  journalUsers =
    log.journalReaders
    ++ lib.optionals (cfg.enable && log.enable && rb.enable) [ rb.user ]
    ++ lib.optionals (cfg.enable && log.enable && lake.enable) [ "surmount-lake" ];
in
{
  config = mkMerge [
    (mkIf (cfg.enable && log.enable) {
      assertions = [
        {
          assertion = log.systemMaxUse != "";
          message = "surmount.logging.systemMaxUse must be a non-empty systemd size (scaffold 1G; not a published guest disk size).";
        }
        {
          assertion = log.runtimeMaxUse != "";
          message = "surmount.logging.runtimeMaxUse must be a non-empty systemd size (scaffold 256M; not a published guest RAM size).";
        }
        {
          assertion = log.maxRetentionSec != "";
          message = "surmount.logging.maxRetentionSec must be a non-empty systemd time (scaffold 30day).";
        }
        {
          assertion = log.rateLimitIntervalSec != "";
          message = "surmount.logging.rateLimitIntervalSec must be a non-empty systemd time (scaffold 30s). Do not set 0.";
        }
        {
          assertion = log.rateLimitBurst != "";
          message = "surmount.logging.rateLimitBurst must be a non-empty integer string (scaffold 50000). Do not set 0.";
        }
        {
          assertion = log.rateLimitIntervalSec != "0" && log.rateLimitIntervalSec != "0s";
          message = "surmount.logging.rateLimitIntervalSec must not be 0 (that disables journal rate limiting).";
        }
        {
          assertion = log.rateLimitBurst != "0";
          message = "surmount.logging.rateLimitBurst must not be 0 (that disables journal rate limiting).";
        }
        {
          assertion =
            builtins.match "[0-9]+" log.rateLimitBurst != null
            && (lib.toInt log.rateLimitBurst) >= 20000;
          message = "surmount.logging.rateLimitBurst must be an integer >= 20000 so torture-test and OOM-start lines are not the first dropped (scaffold 50000; completeness over vacuum).";
        }
        {
          assertion = log.sshdLogLevel != "";
          message = "surmount.logging.sshdLogLevel must be a non-empty OpenSSH LogLevel (scaffold VERBOSE).";
        }
      ];

      # Persistent journal under /var/log/journal. Size + age vacuum so the
      # paper trail cannot fill the disk (same class as SHC ticket 261).
      # Rate limit is explicit and kept high so the start of an OOM or
      # cgroup kill is still in the journal. Completeness over vacuum.
      services.journald.extraConfig = ''
        Storage=persistent
        SystemMaxUse=${log.systemMaxUse}
        RuntimeMaxUse=${log.runtimeMaxUse}
        MaxRetentionSec=${log.maxRetentionSec}
        RateLimitIntervalSec=${log.rateLimitIntervalSec}
        RateLimitBurst=${log.rateLimitBurst}
        ForwardToSyslog=no
      '';

      # Auth failures and disconnects without passwords. Not DEBUG3.
      services.openssh.settings.LogLevel = log.sshdLogLevel;

      # Per-unit burst so nix-daemon start/stop/OOM lines survive a flood.
      # Do not Nice= this unit here (remote-builder owns Nice when enabled).
      systemd.services.nix-daemon.serviceConfig = {
        LogRateLimitIntervalSec = "30s";
        LogRateLimitBurst = 50000;
      };
    })

    (mkIf (cfg.enable && log.enable && journalUsers != [ ]) {
      # systemd-journal can read the system journal without sudo.
      # Do not add nixbuilder to wheel. Do not Nice= mail units here.
      users.users = lib.mkMerge (
        map (name: {
          ${name}.extraGroups = [ "systemd-journal" ];
        }) journalUsers
      );
    })
  ];
}
