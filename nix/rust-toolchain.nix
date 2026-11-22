# Crane / `nix develop` rustc. Pass nixpkgs-rust, not the mail-host channel.
# nixpkgs-rust is pinned to 981724d1e07192596b94a2dcd4bd3c30139cc508.
# That commit sets rustcVersion = "1.100.0" in
# pkgs/development/compilers/rust/1_100.nix. Not a newer compiler.
# Not the mail-host channel (nixos-26.05 pkgs.rustc 1.95.0).
# Prior lock 342414c3be9098a87dd50b560e96842a1c79cb9f was rustc 1.99.1.
# A forgotten pkgsRust ? pkgs argument used to select mail-host rustc 1.95.0.
# That path throws here instead of compiling.
{ pkgs }:
assert pkgs.lib.assertMsg (pkgs.rustc.version == "1.100.0")
  "nix/rust-toolchain.nix: rustc ${pkgs.rustc.version} is not 1.100.0. Pass the nixpkgs-rust package set. Do not pass mail-host pkgs (rustc 1.95.0).";
pkgs.symlinkJoin {
  name = "surmount-rust-${pkgs.rustc.version}";
  paths = [
    pkgs.rustc
    pkgs.cargo
    pkgs.clippy
    pkgs.rustfmt
  ];
}
