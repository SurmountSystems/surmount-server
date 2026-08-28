# Hermetic crane build of crates/surmount-mail-import.
# Maildir++ import via Vandelay. Not a writeShellApplication wrapping bash.
# Never calls stalwart-cli import. Never logs STALWART_TOKEN.
#
# Call as: pkgs.callPackage ./surmount-mail-import.nix { inherit craneLib; }

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
    pname = "surmount-mail-import";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-mail-import";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoTestExtraArgs = "-p surmount-mail-import";
    meta = {
      description = "Operator Maildir++ import into Stalwart via Vandelay";
      mainProgram = "surmount-mail-import-maildir";
      license = lib.licenses.unlicense;
    };
  }
)
