# Eternal Terminal server for operator sessions that survive sleep and
# network change. Stock nixpkgs services.eternal-terminal. Not a Mullvad
# replacement. Not niced (sshd class). Journal clues stay on (silent=false).
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  et = cfg.hardening.eternalTerminal;
  inherit (lib) mkIf mkMerge;
in
{
  config = mkIf (cfg.enable && cfg.hardening.enable && et.enable) (mkMerge [
    {
      services.eternal-terminal = {
        enable = true;
        port = et.port;
        # 2 records connection open and close above the previous 1.
        # 9 is not used: this pass did not read etserver for secret fields
        # at the top of the 0-9 scale. silent stays false.
        verbosity = 2;
        silent = false;
      };

      networking.firewall.allowedTCPPorts = [ et.port ];

      environment.systemPackages = [
        pkgs.eternal-terminal
        pkgs.tmux
      ];

      # Operator access path: do not set Nice= or MemoryMax (sshd class).
      # Journal burst so login and disconnect lines survive a flood.
      systemd.services.eternal-terminal.serviceConfig = {
        SyslogIdentifier = "eternal-terminal";
        LogRateLimitIntervalSec = "30s";
        LogRateLimitBurst = 50000;
      };
    }
  ]);
}
