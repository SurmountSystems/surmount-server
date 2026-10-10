# Consume github:SurmountSystems/grok-oss flake package (bin/grok-oss).
# Call as: pkgs.callPackage ./grok-oss.nix { inherit grokOssFlake system rustToolchain; }
# Fail-closed: missing packages.grok-oss throws. Do not enable the NixOS
# module without this package.
# pkgs stays the host channel for C libraries. rustToolchain is
# nixpkgs-rust (rustc 1.99.0). Do not use host pkgs.rustc / pkgs.cargo.

{
  grokOssFlake,
  system,
  pkgs,
  lib,
  runCommand,
  rustToolchain,
}:
let
  # Rewrite leftover stdenv.isLinux / stdenv.isDarwin to hostPlatform
  # before import. A tip that already uses hostPlatform is unchanged.
  # Do not evaluate grokOssFlake.packages (the old form warns).
  patchedSrc = runCommand "grok-oss-flake-hostplatform" { } ''
    cp -a ${grokOssFlake.outPath} "$out"
    chmod -R u+w "$out"
    find "$out" -type f -name '*.nix' -print0 | xargs -0 sed -i \
      -e 's/pkgs\.stdenv\.isLinux/pkgs.stdenv.hostPlatform.isLinux/g' \
      -e 's/pkgs\.stdenv\.isDarwin/pkgs.stdenv.hostPlatform.isDarwin/g' \
      -e 's/stdenv\.isLinux/stdenv.hostPlatform.isLinux/g' \
      -e 's/stdenv\.isDarwin/stdenv.hostPlatform.isDarwin/g'
  '';
  craneLib = (grokOssFlake.inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;
  built = import "${patchedSrc}/flake/grok-oss.nix" {
    inherit pkgs lib craneLib;
    self = grokOssFlake;
  };
in
built.grok-oss or built.default
  or (throw "grok-oss flake input has no packages.${system}.grok-oss (fail-closed; do not set surmount.grokOss.enable without the package)")
