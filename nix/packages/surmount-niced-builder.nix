# Hermetic crane build of crates/surmount-niced-builder.
# Call as: pkgs.callPackage ./surmount-niced-builder.nix { inherit craneLib; }
# MemoryMax on the host still lives in modules/remote-builder.nix.

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
    pname = "surmount-niced-builder";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [
      pkg-config
      pkgs.util-linux
      pkgs.coreutils
    ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-niced-builder";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-niced-builder";
      meta = {
        description = "Niced ssh-ng nix-daemon helper (not a mail unit wrapper)";
        mainProgram = "surmount-niced-builder";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-niced-builder";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-niced-builder \
      --prefix PATH : ${
        lib.makeBinPath [
          pkgs.systemd
          pkgs.util-linux
          pkgs.coreutils
        ]
      }
  '';
  meta = unwrapped.meta;
}
