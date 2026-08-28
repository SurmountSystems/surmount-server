# Hermetic crane build of crates/surmount-rekey (just rekey).
# age crate (rage library) plus gpg on PATH. Not writeShellApplication.
#
# Call as: pkgs.callPackage ./surmount-rekey.nix { inherit craneLib; }

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
    pname = "surmount-rekey";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-rekey";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-rekey";
      meta = {
        description = "SHC backup re-key paste (PGP wrap)";
        mainProgram = "surmount-rekey";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-rekey";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-rekey \
      --suffix PATH : ${lib.makeBinPath [ pkgs.gnupg ]}
  '';
  meta = unwrapped.meta;
}
