# Crane / `nix develop` rustc. Pass nixpkgs-rust, not the mail-host channel.
# nixpkgs-rust is pinned to fd400cc205c104a499657a82680b6a3eaa8b7d8e.
# That commit sets rustcVersion = "1.100.0" in
# pkgs/development/compilers/rust/1_100.nix. It is open as
# NixOS/nixpkgs pull 586088 against staging, not merged to master.
# Read 2026-11-16: master still has rustcVersion = "1.99.1" in
# pkgs/development/compilers/rust/1_99.nix. Not the 1.100 beta.
# Not nixos-unstable. Prior lock
# 9ecc0a672e9ef2ca4eec8e87dbee653607a7d332 was rustc 1.99.1.
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
