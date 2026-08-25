# Hermetic crane build of crates/surmount-domain-audit.
# Read-only DNS/TLS/mailbox posture. Not a writeShellApplication wrapping bash.
#
# Call as: pkgs.callPackage ./surmount-domain-audit.nix { inherit craneLib; }

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
    pname = "surmount-domain-audit";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-domain-audit";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    # Hermetic PATH-fixture tests only. Never live DNS as CI green.
    cargoTestExtraArgs = "-p surmount-domain-audit";
    meta = {
      description = "Read-only DNS, TLS, and mailbox posture audit";
      mainProgram = "surmount-domain-audit";
      license = lib.licenses.unlicense;
    };
  }
)
