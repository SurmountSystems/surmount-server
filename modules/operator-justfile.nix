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
    environment.systemPackages = [ pkgs.just ];
    environment.etc.${etcRel}.source = ../contrib/guest-root-justfile;
    systemd.tmpfiles.rules = [
      "L+ /root/justfile - - - - ${etcAbs}"
    ];
  };
}
