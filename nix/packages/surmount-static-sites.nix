# Hermetic crane build of crates/surmount-static-sites.
# Call as: pkgs.callPackage ./surmount-static-sites.nix { inherit craneLib; }

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
    pname = "surmount-static-sites";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-static-sites";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-static-sites";
      meta = {
        description = "Publish apex/www and proven DS3018xs extra vhost trees";
        mainProgram = "surmount-deploy-static-sites";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-static-sites";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-deploy-static-sites \
      --prefix PATH : ${
        lib.makeBinPath [
          pkgs.rsync
          pkgs.openssh
        ]
      }
    wrapProgram $out/bin/surmount-sync-static-sites \
      --prefix PATH : ${
        lib.makeBinPath [
          pkgs.rsync
          pkgs.openssh
        ]
      }
  '';
  meta = unwrapped.meta;
}
