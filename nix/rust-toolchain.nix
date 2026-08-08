# Crane toolchain for management-ui (Leptos SSR wants rustc >= 1.88).
# Host channel nixos-26.05 ships rustc 1.95 and rustPackages_1_95 (1.88 set
# removed). Pin the channel's rustPackages so crane and `nix develop` match.
{ pkgs }:
pkgs.symlinkJoin {
  name = "surmount-rust-1.95";
  paths = with pkgs.rustPackages_1_95; [
    rustc
    cargo
    clippy
    rustfmt
  ];
}
