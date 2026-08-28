# Crane / `nix develop` rustc. Pass nixpkgs-rust (nixos-unstable), not the
# mail-host channel: nixos-26.05 is still 1.95. Locked unstable is 1.97.1
# (2026-08-26). Rust 1.98.0 is out (2026-08-20) but not in this nixpkgs yet.
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
