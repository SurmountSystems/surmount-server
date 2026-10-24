# grok-oss is not in the mail-host server closure.
# Options remain in options.nix so eval stays defined. Enable must stay
# false. Host-local must not turn this on. This module never installs
# pkgs.grok-oss, tmux-for-grok, or a grok user.

{
  config,
  lib,
  ...
}:
let
  go = config.surmount.grokOss;
in
{
  config = {
    assertions = [
      {
        assertion = !go.enable;
        message = "surmount.grokOss.enable must stay false: grok-oss is not in the mail-host server closure. Do not re-enable from host-local.";
      }
    ];
  };
}
