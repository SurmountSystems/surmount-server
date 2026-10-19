# Optional Tau coding agent on the mail host. Default off.
# Enable from host-local only. No systemd unit. No extension packages.
# When enable is true, install inputs.tau.packages.${system}.tau only
# (pkgs.tau from that flake attr). Default-off does not reference it.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount.tau;
  inherit (lib) mkIf mkOption types;
  system = pkgs.stdenv.hostPlatform.system;
in
{
  options.surmount.tau.enable = mkOption {
    type = types.bool;
    default = false;
    description = ''
      Install bin/tau from the locked SurmountSystems/tau flake input
      (packages.tau, upstream nix/pkgs/tau.nix). Default false. The
      mail closure does not gain tau while this stays false. Does not
      install extension packages. Does not add a systemd unit. Enable
      only from private host-local.
    '';
  };

  config = mkIf cfg.enable {
    environment.systemPackages = [
      (pkgs.tau or (throw "tau flake input has no packages.${system}.tau (fail-closed)"))
    ];
  };
}
