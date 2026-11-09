# Guest /root/justfile: diagnose wrappers. Source is contrib/guest-root-justfile
# in this repo. Do not hand-place a justfile only on one host.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf;
  etcRel = "surmount/root-justfile";
  etcAbs = "/etc/${etcRel}";
in
{
  config = mkIf cfg.enable {
    # No service executes just. The recipe file stays. The just
    # package builds with cargo, and that cargo build input is the
    # interpreter. Do not install the package on this mail host.
    environment.etc.${etcRel}.source = ../contrib/guest-root-justfile;
    systemd.tmpfiles.rules = [
      "L+ /root/justfile - - - - ${etcAbs}"
    ];
  };
}
