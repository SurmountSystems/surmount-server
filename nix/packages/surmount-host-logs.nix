# Hermetic crane build of crates/surmount-host-logs.
# Host journal reader only (follow + --status). Journald stays source of truth.
# Wrap PATH with systemd so journalctl/systemctl resolve without sudo.
#
# Call as: pkgs.callPackage ./surmount-host-logs.nix { inherit craneLib; }

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
    pname = "surmount-host-logs";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-host-logs";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-host-logs";
      meta = {
        description = "Read the host systemd journal (follow and --status). No sudo.";
        mainProgram = "surmount-host-logs";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-host-logs";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-host-logs \
      --prefix PATH : ${lib.makeBinPath [ pkgs.systemd ]}
  '';
  meta = {
    description = "Read the host systemd journal (follow and --status). No sudo.";
    mainProgram = "surmount-host-logs";
    license = lib.licenses.unlicense;
  };
}
