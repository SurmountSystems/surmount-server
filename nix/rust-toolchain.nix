# Crane / `nix develop` rustc. Pass nixpkgs-rust (nixos-unstable), not the
# mail-host channel: nixos-26.05 rustc is still 1.95.0. Locked unstable
# rustc is 1.98.1 (nixpkgs-rust rev c59305ba, checked 2026-10-03).
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
