# Hermetic crane build of crates/surmount-acme-namecheap (all bins).
# Not a writeShellApplication wrapping bash. Credentials stay off this derivation.
#
# Call as: pkgs.callPackage ./surmount-acme-namecheap.nix { inherit craneLib; }

{
  lib,
  pkgs,
  craneLib,
  pkg-config,
}:
let
  rustToolchain = import ../rust-toolchain.nix { inherit pkgs; };
  craneLib' = craneLib.overrideToolchain rustToolchain;

  src = lib.cleanSourceWith {
    src = craneLib'.path ../../crates;
    filter = path: type: craneLib'.filterCargoSources path type;
  };

  commonArgs = {
    inherit src;
    pname = "surmount-acme-namecheap";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-acme-namecheap";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    # Hermetic mock tests only. Never live Namecheap as CI green.
    cargoTestExtraArgs = "-p surmount-acme-namecheap";
    meta = {
      description = "Namecheap DNS-01 ACME hook, dispatch, lab, laptop renew, host-profile render";
      mainProgram = "acme-dns-hook-namecheap";
      license = lib.licenses.unlicense;
    };
  }
)
