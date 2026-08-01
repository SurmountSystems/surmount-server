# Hermetic crane build of crates/management-ui.
# Call as: pkgs.callPackage ./management-ui.nix { inherit craneLib; }
# or via the flake overlay which injects craneLib.
#
# Leptos 0.8 SSR MSRV is rustc 1.88+. When craneLib is not overridden by the
# flake, this package bumps to rustPackages_1_88 itself.

{
  lib,
  pkgs,
  craneLib,
  pkg-config,
  openssl,
# Optional: cargo artifacts for faster incremental CI later.
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
    pname = "surmount-management-ui";
    version = "0.1.0";
    strictDeps = true;
    nativeBuildInputs = [ pkg-config ];
    # reqwest rustls-tls usually needs no openssl; keep pkg-config light.
    # If a dep pulls openssl-sys, add openssl to buildInputs.
    buildInputs = [ ];
    # Workspace member
    cargoExtraArgs = "-p surmount-management-ui";
  };

  cargoArtifacts = craneLib'.buildDepsOnly (
    commonArgs
    // {
      # deps-only still needs the lockfile present in src
    }
  );
in
craneLib'.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    meta = {
      description = "Surmount management UI (Axum + Leptos SSR)";
      mainProgram = "surmount-management-ui";
    };
  }
)
