# Splora git pin the systems status surface must observe.
# The management UI unit exports environment. Host status and the
# systems dashboard read that same string. The locked flake input rev
# has to match expected or this file throws.
{ lib, sploraRev }:
let
  expected = "1a50824e06dec0680ea7e414255970e8618ddf13";
in
if !lib.isString sploraRev || sploraRev != expected then
  throw "splora status pin is ${toString sploraRev}, expected ${expected}"
else
  {
    rev = sploraRev;
    environment = "SURMOUNT_SPLORA_REV=${sploraRev}";
  }
