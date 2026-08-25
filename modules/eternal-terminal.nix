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
        verbosity = 1;
        silent = false;
      };

      networking.firewall.allowedTCPPorts = [ et.port ];

      environment.systemPackages = [
        pkgs.eternal-terminal
        pkgs.tmux
      ];

      # Operator access path: do not set Nice= or MemoryMax (sshd class).
      systemd.services.eternal-terminal.serviceConfig.SyslogIdentifier = "eternal-terminal";
    }
  ]);
}
