# Hermetic crane build of crates/surmount-host-cutover.
# Host cutover driver + material inventory. Not a writeShellApplication
# wrapping bash. Same rust-toolchain as management-ui (nix/rust-toolchain.nix).
#
# Call as: pkgs.callPackage ./surmount-host-cutover.nix { inherit craneLib; }

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
      || (builtins.match ".*authorized_keys$" path != null)
      || (builtins.match ".*\\.(nix|toml|txt|md)$" path != null);
  };

  commonArgs = {
    inherit src;
    pname = "surmount-host-cutover";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-host-cutover";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-host-cutover";
      meta = {
        description = "Host cutover driver (inventory, fragments, secrets/deploy plan)";
        mainProgram = "surmount-host-cutover";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-host-cutover";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-host-cutover \
      --prefix PATH : ${
        lib.makeBinPath [
          pkgs.openssh
          pkgs.rsync
        ]
      }
  '';
  meta = unwrapped.meta;
}
