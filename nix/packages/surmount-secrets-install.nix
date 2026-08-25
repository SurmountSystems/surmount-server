# Hermetic crane build of crates/surmount-secrets-install.
# Domain A to Domain B install bridge + Vaultwarden export-to-staging.
# Not a writeShellApplication wrapping bash. Never logs secret values.
# Hermetic --dest-root / --from-fixture tests only. Never live keyring as CI green.
#
# Call as: pkgs.callPackage ./surmount-secrets-install.nix { inherit craneLib; }

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
    pname = "surmount-secrets-install";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-secrets-install";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoTestExtraArgs = "-p surmount-secrets-install";
    meta = {
      description = "Domain A to Domain B deploy-secrets install bridge";
      mainProgram = "secrets-install-host";
      license = lib.licenses.unlicense;
    };
  }
)
