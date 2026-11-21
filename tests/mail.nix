# The only VM check in this file was removed.
# The upstream NixOS runner defines its script option with no default, then
# evaluates that option while building the driver. Every check lived in that
# script. No maintained Rust or C replacement for that runner was cited.
# This file does not start that runner.
#
# flake.nix still imports this path for the optional mail VM check.
# That import is outside this edit. Evaluating the import throws.
{
  pkgs,
  # Passed by flake.nix. Unused after the check was removed.
  lib,
  nixosTest,
  surmountModules,
  managementUi,
}:
builtins.throw ''
  The mail VM check was removed from tests/mail.nix.
  The upstream NixOS VM runner requires a script this tree does not keep.
  No Rust or C replacement for that runner was cited.
''
