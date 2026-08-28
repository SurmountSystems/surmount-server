# Hermetic crane build of crates/surmount-stalwart-ops.
# DKIM, free :443, mail-plane TLS, tokens, recovery. Not a writeShellApplication wrapping bash.
# Never logs STALWART_TOKEN. Hermetic fake-CLI tests only. Never live Stalwart as CI green.
#
# Call as: pkgs.callPackage ./surmount-stalwart-ops.nix { inherit craneLib; }

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
    pname = "surmount-stalwart-ops";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-stalwart-ops";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoTestExtraArgs = "-p surmount-stalwart-ops";
    meta = {
      description = "Stalwart operator drivers (DKIM, free :443, mail-plane TLS, tokens, recovery)";
      mainProgram = "register-dkim";
      license = lib.licenses.unlicense;
    };
  }
)
