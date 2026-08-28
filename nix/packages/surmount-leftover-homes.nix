# Hermetic crane build of crates/surmount-leftover-homes.
# Call as: pkgs.callPackage ./surmount-leftover-homes.nix { inherit craneLib; }

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
    filter =
      path: type: (craneLib'.filterCargoSources path type) || (builtins.match ".*\\.sh$" path != null);
  };

  commonArgs = {
    inherit src;
    pname = "surmount-leftover-homes";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [
      pkg-config
      pkgs.git
    ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-leftover-homes";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-leftover-homes";
      meta = {
        description = "Refuse leftover repo .agents / .grok agent-home mkdir paths";
        mainProgram = "surmount-leftover-homes";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-leftover-homes";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-leftover-homes \
      --prefix PATH : ${lib.makeBinPath [ pkgs.git ]}
  '';
  meta = unwrapped.meta;
}
