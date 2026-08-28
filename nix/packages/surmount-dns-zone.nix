# Hermetic crane build of crates/surmount-dns-zone.
# Namecheap zone helper. Not a writeShellApplication wrapping bash.
# Credentials stay off this derivation. Hermetic mock tests only.
#
# Call as: pkgs.callPackage ./surmount-dns-zone.nix { inherit craneLib; }

{
  lib,
  pkgs,
  pkgsRust ? pkgs,
  craneLib,
  pkg-config,
}:
let
  rustToolchain = import ../rust-toolchain.nix { pkgs = pkgsRust; };
  craneLib' = craneLib.overrideToolchain rustToolchain;

  src = lib.cleanSourceWith {
    src = craneLib'.path ../..;
    filter = path: type: craneLib'.filterCargoSources path type;
  };

  commonArgs = {
    inherit src;
    pname = "surmount-dns-zone";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-dns-zone";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    # Hermetic mock tests only. Never live Namecheap as CI green.
    cargoTestExtraArgs = "-p surmount-dns-zone";
    meta = {
      description = "Namecheap forward-zone helper (A/AAAA/TXT/CAA/MX). Dry-run default.";
      mainProgram = "surmount-dns-zone";
      license = lib.licenses.unlicense;
    };
  }
)
