# Hermetic crane build of crates/surmount-shc (SHC customer user-api client).
# rDNS PTR + support tickets. Not a writeShellApplication wrapping bash.
# Credentials stay off this derivation (Domain A / Domain B env file).
#
# Call as: pkgs.callPackage ./surmount-shc.nix { inherit craneLib; }

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
    pname = "surmount-shc";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-shc";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    # Hermetic mock tests only. Never live SHC as CI green.
    cargoTestExtraArgs = "-p surmount-shc";
    meta = {
      description = "Sovereign Hybrid Compute customer user-api client (rDNS and tickets)";
      mainProgram = "surmount-shc";
      license = lib.licenses.unlicense;
    };
  }
)
