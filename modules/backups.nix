# restic backup skeleton for mail store + optional site data.
# Disabled until surmount.backups.repository and passwordFile are set.
# One-time restore drills are operator-run; nothing destructive here.

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf;
  backupsOn =
    cfg.enable && cfg.backups.enable && cfg.backups.repository != "" && cfg.backups.passwordFile != "";
in
{
  config = mkIf backupsOn {
    services.restic.backups.surmount = {
      initialize = true;
      repository = cfg.backups.repository;
      passwordFile = cfg.backups.passwordFile;
      paths = [
        cfg.mailDataDir
        cfg.stateDir
      ]
      ++ cfg.backups.paths;
      timerConfig = {
        OnCalendar = "daily";
        Persistent = true;
        RandomizedDelaySec = "1h";
      };
      # Prune policy is conservative; tighten once retention needs are clear.
      pruneOpts = [
        "--keep-daily 7"
        "--keep-weekly 4"
        "--keep-monthly 6"
      ];
    };
  };
}
