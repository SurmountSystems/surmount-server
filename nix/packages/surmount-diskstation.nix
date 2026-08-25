# Hermetic crane build of crates/surmount-diskstation.
# Call as: pkgs.callPackage ./surmount-diskstation.nix { inherit craneLib; }

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
    pname = "surmount-diskstation";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-diskstation";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-diskstation";
      meta = {
        description = "DiskStation AFP mount, mDNS discover, MailPlus uid copy";
        mainProgram = "surmount-diskstation-afp-mount";
        license = lib.licenses.unlicense;
      };
    }
  );
  pathPkgs = [
    pkgs.glib
    pkgs.avahi
    pkgs.libsecret
    pkgs.openssh
    pkgs.rsync
  ];
in
pkgs.symlinkJoin {
  name = "surmount-diskstation";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    for b in \
      surmount-diskstation-afp-mount \
      surmount-diskstation-discover \
      surmount-copy-mailplus-uid \
      surmount-fix-public-dashboard
    do
      wrapProgram $out/bin/$b \
        --prefix PATH : ${lib.makeBinPath pathPkgs}
    done
  '';
  meta = unwrapped.meta;
}
