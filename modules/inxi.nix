# Host convenience: pkgs.inxi so operators can SSH and read hardware
# topology without sudo. Not a critical service.
# A deploy-host switch is required before the package exists on the box.
# Laptop inxi is a different machine. Do not copy those numbers here.
# Do not set guest nix max-jobs from raw guest nproc.

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
    environment.systemPackages = [ pkgs.inxi ];
  };
}
