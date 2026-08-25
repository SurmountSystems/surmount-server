# Hermetic crane build of crates/surmount-deploy-host.
# Operator deploy driver + post-switch smoke. Not a writeShellApplication
# wrapping bash. Same rust-toolchain as management-ui (nix/rust-toolchain.nix).
#
# Call as: pkgs.callPackage ./surmount-deploy-host.nix { inherit craneLib; }

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
    filter =
      path: type:
      (craneLib'.filterCargoSources path type)
      || (builtins.match ".*authorized_keys$" path != null)
      || (builtins.match ".*\\.(nix|toml|txt|md)$" path != null);
  };

  commonArgs = {
    inherit src;
    pname = "surmount-deploy-host";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-deploy-host";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-deploy-host";
      meta = {
        description = "Operator-driven NixOS deploy driver and post-switch smoke";
        mainProgram = "surmount-deploy-host";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-deploy-host";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-deploy-host \
      --prefix PATH : ${
        lib.makeBinPath [
          pkgs.rsync
          pkgs.openssh
        ]
      }
    wrapProgram $out/bin/surmount-deploy-host-post-switch-smoke \
      --prefix PATH : ${
        lib.makeBinPath [
          pkgs.systemd
          pkgs.curl
          pkgs.iproute2
        ]
      }
  '';
  meta = unwrapped.meta;
}
