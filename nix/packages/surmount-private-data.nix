# Hermetic crane build of crates/surmount-private-data.
# Call as: pkgs.callPackage ./surmount-private-data.nix { inherit craneLib; }
# or via the flake overlay which injects craneLib.
#
# Same rust-toolchain as management-ui (nix/rust-toolchain.nix).

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
      path: type:
      (craneLib'.filterCargoSources path type)
      || (builtins.match ".*\\.(html|css|js|svg|png|toml|json|txt|md)$" path != null);
  };

  commonArgs = {
    inherit src;
    pname = "surmount-private-data";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [
      pkg-config
      pkgs.git
    ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-private-data";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-private-data";
      meta = {
        description = "Surmount private-data pattern scanner (git admission)";
        mainProgram = "surmount-private-data";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-private-data";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-private-data \
      --prefix PATH : ${lib.makeBinPath [ pkgs.git ]}
  '';
  meta = unwrapped.meta;
}
