# Rust replacement for systemd 262.2 src/core/generate-builtin-units.py.
#
# Toolchain pin, quoted from flake.lock nixpkgs-rust (do not invent
# another compiler):
#   rev 981724d1e07192596b94a2dcd4bd3c30139cc508
#   narHash sha256-gjAmGk/n8OS7bAvGqUfTC7YTN8HgkfQGzgpjAR77k84=
# nix/rust-toolchain.nix asserts that this package set's rustc.version
# is 1.100.0. Mail-host pkgs.rustc is 1.95.0. This file imports the
# pinned set itself so the systemd native build does not depend on
# the mail-host channel rustc.
#
# The default checkPhase is cargoCheckHook (`cargo test`). doCheck
# stays true. The crate has no crates.io dependencies.
{ system }:
let
  nixpkgsRust = fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/981724d1e07192596b94a2dcd4bd3c30139cc508.tar.gz";
    sha256 = "sha256-gjAmGk/n8OS7bAvGqUfTC7YTN8HgkfQGzgpjAR77k84=";
  };
  pkgsRust = import nixpkgsRust {
    inherit system;
    config = { };
    overlays = [ ];
  };
  # Forces the rustc 1.100.0 assert. Unused bindings are lazy.
  asserted = import ../rust-toolchain.nix { pkgs = pkgsRust; };
  lib = pkgsRust.lib;
  crate = ../../crates/surmount-systemd-builtin-units;
in
pkgsRust.rustPlatform.buildRustPackage {
  pname = "surmount-systemd-builtin-units";
  version = "0.1.0";
  src = lib.cleanSource crate;
  cargoLock.lockFile = crate + "/Cargo.lock";
  strictDeps = true;
  doCheck = true;
  # On PATH during the build so the 1.100.0 assert is an input.
  # rustPlatform already runs cargo and rustc from this same set.
  nativeBuildInputs = [ asserted ];
  meta = {
    description = "systemd 262.2 generate-builtin-units replacement (rustc ${asserted.name})";
    mainProgram = "surmount-systemd-builtin-units";
    license = lib.licenses.unlicense;
    platforms = lib.platforms.linux;
  };
}
