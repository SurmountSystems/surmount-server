# Hermetic crane build of crates/surmount-host-probe
# (inxi-host, btop-host, tls-hybrid).
# Call as: pkgs.callPackage ./surmount-host-probe.nix { inherit craneLib; }

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
    pname = "surmount-host-probe";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-host-probe";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  unwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoTestExtraArgs = "-p surmount-host-probe";
      meta = {
        description = "Host probes: inxi (no TTY), btop (TTY), hybrid TLS";
        mainProgram = "surmount-inxi-host";
        license = lib.licenses.unlicense;
      };
    }
  );
in
pkgs.symlinkJoin {
  name = "surmount-host-probe";
  paths = [ unwrapped ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/bin/surmount-inxi-host \
      --prefix PATH : ${lib.makeBinPath [ pkgs.openssh ]}
    wrapProgram $out/bin/surmount-btop-host \
      --prefix PATH : ${lib.makeBinPath [ pkgs.openssh ]}
    wrapProgram $out/bin/surmount-tls-hybrid \
      --prefix PATH : ${lib.makeBinPath [ pkgs.openssl ]}
    wrapProgram $out/bin/surmount-et \
      --prefix PATH : ${lib.makeBinPath [ pkgs.eternal-terminal pkgs.openssh ]}
  '';
  meta = unwrapped.meta;
}
