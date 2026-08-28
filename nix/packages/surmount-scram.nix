# Hermetic crane build of crates/surmount-scram.
# Call as: pkgs.callPackage ./surmount-scram.nix { inherit craneLib; }

{
  lib,
  pkgs,
  pkgsRust ? pkgs,
  craneLib,
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
    pname = "surmount-scram";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-scram";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoTestExtraArgs = "-p surmount-scram";
    meta = {
      description = "SIGKILL builder hogs when host MemAvailable is below 16 GiB";
      mainProgram = "surmount-scram";
    };
  }
)
