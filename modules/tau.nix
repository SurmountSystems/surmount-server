# tau is not in the mail-host server closure.
# The enable option stays so leftover host-local can be refused closed.
# This module never installs pkgs.tau and never adds a daemon.

{
  config,
  lib,
  ...
}:
let
  inherit (lib) mkOption types;
in
{
  options.surmount.tau.enable = mkOption {
    type = types.bool;
    default = false;
    description = ''
      Must stay false. Tau is not in the mail-host server closure.
      The flake does not take a tau input. Host-local must not enable this.
    '';
  };

  config = {
    assertions = [
      {
        assertion = !config.surmount.tau.enable;
        message = "surmount.tau.enable must stay false: tau is not in the mail-host server closure. Do not re-enable from host-local.";
      }
    ];
  };
}
