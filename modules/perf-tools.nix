# Host convenience: pkgs.fio from nixpkgs so sequential vs random
# direct-I/O jobs can run from this flake's module set.
# Not a critical service. Do not write the system disk; jobs belong on
# a dedicated HDD mount. A deploy-host switch is required before the
# binary exists on the guest PATH. Until then, nix eval / nix run of
# pkgs.fio from this flake is the fallback.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf;
in
{
  config = mkIf cfg.enable {
    environment.systemPackages = [ pkgs.fio ];
  };
}
