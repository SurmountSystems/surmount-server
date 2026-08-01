# Crane toolchain for management-ui (Leptos SSR wants rustc >= 1.88).
# nixpkgs default rustc on 25.05 is 1.86; use rustPackages_1_88 without
# rebasing the whole host channel.
{ pkgs }:
pkgs.symlinkJoin {
  name = "surmount-rust-1.88";
  paths = with pkgs.rustPackages_1_88; [
    rustc
    cargo
    clippy
    rustfmt
  ];
}
