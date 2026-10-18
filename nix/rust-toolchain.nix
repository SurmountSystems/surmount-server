# Crane / `nix develop` rustc. Pass nixpkgs-rust, not the mail-host channel.
# nixos-26.05 rustc is still 1.95.0. nixpkgs-rust is pinned to c46e6c9a
# (nixpkgs master 2026-10-18, rustc 1.99.0, pkgs.arti 2.7.0). Upstream
# stable on 2026-10-17 is 1.99.1, and nixpkgs has no 1.99.1. Not
# nixos-unstable (still 1_98.nix on that date). Not a beta.
{ pkgs }:
pkgs.symlinkJoin {
  name = "surmount-rust-${pkgs.rustc.version}";
  paths = [
    pkgs.rustc
    pkgs.cargo
    pkgs.clippy
    pkgs.rustfmt
  ];
}
