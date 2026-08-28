# Namecheap DNS-01 external-hook (Rust bin, not a bash wrap).
#
# Keep pname/mainProgram acme-dns-hook-namecheap so management-ui can set
# acme.dnsHookPath to a fixed Nix store path. Credentials stay laptop custody
# then secrets-install-host. Never embed API credentials in this store package.
#
# Call as: pkgs.callPackage ./acme-dns-hook-namecheap.nix { inherit craneLib; }

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
    pname = "acme-dns-hook-namecheap";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-acme-namecheap --bin acme-dns-hook-namecheap";
  };

  cargoArtifacts = craneLib'.buildDepsOnly (
    commonArgs
    // {
      cargoExtraArgs = "-p surmount-acme-namecheap";
    }
  );
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    cargoTestExtraArgs = "-p surmount-acme-namecheap --bin acme-dns-hook-namecheap";
    meta = {
      description = "Namecheap DNS-01 ACME external-hook (Surmount helper; Rust)";
      homepage = "https://github.com/SurmountSystems/surmount-server";
      license = lib.licenses.unlicense;
      mainProgram = "acme-dns-hook-namecheap";
      platforms = lib.platforms.linux;
    };
  }
)
