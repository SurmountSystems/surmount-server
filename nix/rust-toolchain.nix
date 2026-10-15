# Crane / `nix develop` rustc. Pass nixpkgs-rust, not the mail-host channel.
# nixos-26.05 rustc is still 1.95.0. nixpkgs-rust is pinned to c9f19b6
# (rustc 1.99.0, pkgs.arti 2.6.0), not floating nixos-unstable.
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
