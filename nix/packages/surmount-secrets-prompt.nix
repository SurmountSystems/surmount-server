# Hermetic crane build of crates/surmount-secrets-prompt.
# Domain A no-echo intake into private staging (optional secret-tool).
# Not a writeShellApplication wrapping bash. Never logs secret values.
# Hermetic batch tests only. Never live keyring as CI green.
#
# Call as: pkgs.callPackage ./surmount-secrets-prompt.nix { inherit craneLib; }

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
    pname = "surmount-secrets-prompt";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [
      pkg-config
      pkgs.bash
    ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-secrets-prompt";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoTestExtraArgs = "-p surmount-secrets-prompt";
    meta = {
      description = "Interactive no-echo Domain A secret intake into private staging";
      mainProgram = "surmount-secrets-prompt";
      license = lib.licenses.unlicense;
    };
  }
)
