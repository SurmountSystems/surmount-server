# Gate for apps.<system>.deploy-dry-run.program.
#
# nix/just-recipe-apps.nix calls this with the plain app and the flake
# already in scope (self). withFlake does not call getFlake.
# Standalone -A ok of tests/dry-run-reveals-deploy.nix still calls
# getFlake once. Do not import this file from the mail-vps module list.
#
# A call that omits app imports just-recipe-apps.nix and wraps that
# file's deploy-dry-run program. just-recipe-apps.nix must pass app.
# Omitting app from that file recurses.
{ self, system, app ? null }:
let
  checks = import ../tests/dry-run-reveals-deploy.nix;
  gate = (checks.withFlake self).ok;
  chosen =
    if builtins.isAttrs app then
      app
    else if app == null then
      (import ./just-recipe-apps.nix { inherit self system; }).deploy-dry-run
    else
      throw "deploy-dry-run-check.nix app must be an attrset or null";
in
{
  deploy-dry-run = chosen // {
    program = builtins.seq gate chosen.program;
  };
}
