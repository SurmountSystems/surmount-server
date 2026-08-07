# Hermetic crane build of crates/surmount-e2e (local + host e2e binaries).
# Runtime wrappers add cargo/curl/openssl so `nix run .#e2e` can orchestrate
# host-tree cargo tests from the operator cwd (not a NixOS VM).
#
# Call as: pkgs.callPackage ./surmount-e2e.nix { inherit craneLib; }

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
      || (builtins.match ".*\\.(html|css|js|svg|png|toml)$" path != null);
  };

  commonArgs = {
    inherit src;
    pname = "surmount-e2e";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ ];
    cargoExtraArgs = "-p surmount-e2e";
  };

  cargoArtifacts = craneLib'.buildDepsOnly commonArgs;

  e2eUnwrapped = craneLib'.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      # Pure helper unit tests run at build time (hermetic; no VPS).
      cargoTestExtraArgs = "-p surmount-e2e --lib";
      meta = {
        description = "Surmount end-to-end runners (local hermetic + host probes)";
        mainProgram = "surmount-e2e";
        # Matches crates/ Cargo.toml SPDX: Unlicense (public domain dedication).
        license = lib.licenses.unlicense;
      };
    }
  );

  runtimeLocal = [
    rustToolchain
    pkgs.curl
    pkgs.openssl
    pkgs.coreutils
    pkgs.findutils
    pkgs.gnugrep
    pkgs.gnused
  ];

  runtimeHost = [
    pkgs.curl
    pkgs.openssl
    pkgs.coreutils
    pkgs.findutils
  ];
in
{
  # Unwrapped crane package (both bins).
  unwrapped = e2eUnwrapped;

  # Local comprehensive e2e: cargo + curl on PATH; run from repo root.
  e2e = pkgs.symlinkJoin {
    name = "surmount-e2e";
    paths = [ e2eUnwrapped ];
    nativeBuildInputs = [ pkgs.makeWrapper ];
    postBuild = ''
      wrapProgram $out/bin/surmount-e2e \
        --prefix PATH : ${lib.makeBinPath runtimeLocal}
      # Host bin is also present; wrap lightly for operators who invoke it
      # from this package path. Prefer packages.e2e-host / apps.e2e-host.
      if [ -x $out/bin/surmount-e2e-host ]; then
        wrapProgram $out/bin/surmount-e2e-host \
          --prefix PATH : ${lib.makeBinPath runtimeHost}
      fi
    '';
    meta = {
      description = "Local comprehensive Surmount end-to-end (nix run .#e2e)";
      mainProgram = "surmount-e2e";
      license = lib.licenses.unlicense;
    };
  };

  # Host probes only (never a flake check).
  e2e-host = pkgs.symlinkJoin {
    name = "surmount-e2e-host";
    paths = [ e2eUnwrapped ];
    nativeBuildInputs = [ pkgs.makeWrapper ];
    postBuild = ''
      # Drop local bin so mainProgram / nix run stays host-only.
      rm -f $out/bin/surmount-e2e
      wrapProgram $out/bin/surmount-e2e-host \
        --prefix PATH : ${lib.makeBinPath runtimeHost}
    '';
    meta = {
      description = "Host Surmount end-to-end probes (nix run .#e2e-host; exit 2 without env)";
      mainProgram = "surmount-e2e-host";
      license = lib.licenses.unlicense;
    };
  };
}
