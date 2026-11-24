# Splora git pin the systems status surface must observe.
# The management UI unit exports environment. Host status and the
# systems dashboard read that same string. The locked flake input rev
# has to match expected or this file throws.
{ lib, sploraRev }:
let
  expected = "79e0cae72e6e3d709465929c04866575f0acdd6e";
in
if !lib.isString sploraRev || sploraRev != expected then
  throw "splora status pin is ${toString sploraRev}, expected ${expected}"
else
  {
    rev = sploraRev;
    environment = "SURMOUNT_SPLORA_REV=${sploraRev}";
  }
