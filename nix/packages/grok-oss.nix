# Consume github:SurmountSystems/grok-oss flake package (bin/grok-oss).
# Call as: pkgs.callPackage ./grok-oss.nix { inherit grokOssFlake system; }
# Fail-closed: missing packages.grok-oss throws. Do not enable the NixOS
# module without this package.

{
  grokOssFlake,
  system,
}:
let
  pkgsFor = grokOssFlake.packages.${system} or { };
in
pkgsFor.grok-oss or pkgsFor.default
  or (throw "grok-oss flake input has no packages.${system}.grok-oss (fail-closed; do not set surmount.grokOss.enable without the package)")
