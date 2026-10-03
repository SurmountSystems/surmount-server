# Consume github:SurmountSystems/grok-oss flake package (bin/grok-oss).
# Call as: pkgs.callPackage ./grok-oss.nix { inherit grokOssFlake system; }
# Fail-closed: missing packages.grok-oss throws. Do not enable the NixOS
# module without this package.

{
  grokOssFlake,
  system,
  pkgs,
  lib,
  runCommand,
}:
let
  # The locked grok-oss tip still reads stdenv.isLinux / stdenv.isDarwin.
  # Nixpkgs warns on that. Patch the nix files, then import the package
  # expression. Do not evaluate grokOssFlake.packages (that is the warning).
  patchedSrc = runCommand "grok-oss-flake-hostplatform" { } ''
    cp -a ${grokOssFlake.outPath} "$out"
    chmod -R u+w "$out"
    find "$out" -type f -name '*.nix' -print0 | xargs -0 sed -i \
      -e 's/pkgs\.stdenv\.isLinux/pkgs.stdenv.hostPlatform.isLinux/g' \
      -e 's/pkgs\.stdenv\.isDarwin/pkgs.stdenv.hostPlatform.isDarwin/g' \
      -e 's/stdenv\.isLinux/stdenv.hostPlatform.isLinux/g' \
      -e 's/stdenv\.isDarwin/stdenv.hostPlatform.isDarwin/g'
  '';
  craneLib = grokOssFlake.inputs.crane.mkLib pkgs;
  built = import "${patchedSrc}/flake/grok-oss.nix" {
    inherit pkgs lib craneLib;
    self = grokOssFlake;
  };
in
built.grok-oss or built.default
  or (throw "grok-oss flake input has no packages.${system}.grok-oss (fail-closed; do not set surmount.grokOss.enable without the package)")
