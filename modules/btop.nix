# Host convenience: pkgs.btop so operators can inspect live load via
# Eternal Terminal (`just btop`) or a guest local `just btop`.
# Not a critical service. Do not set Nice= on mail or other product units.
# A deploy-host switch is required before the package exists on the box.
# Operator laptop look lives in ./btop.conf (flat-remix, no theme background).

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf;
  etcRel = "xdg/btop/btop.conf";
  etcAbs = "/etc/${etcRel}";
  wantedUsers = lib.filterAttrs (
    name: user: name == "root" || (user.isNormalUser or false)
  ) config.users.users;
  homeFor = name: user: if name == "root" then "/root" else user.home;
  tmpfilesFor =
    name: user:
    let
      home = homeFor name user;
    in
    [
      "d ${home}/.config 0755 ${name} ${user.group} -"
      "d ${home}/.config/btop 0755 ${name} ${user.group} -"
      "L+ ${home}/.config/btop/btop.conf - - - - ${etcAbs}"
    ];
in
{
  config = mkIf cfg.enable {
    environment.systemPackages = [ pkgs.btop ];
    environment.etc.${etcRel}.source = ./btop.conf;
    systemd.tmpfiles.rules = lib.flatten (lib.mapAttrsToList tmpfilesFor wantedUsers);
  };
}
