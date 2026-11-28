# Compilers from the package set the caller passes.
# Crane, the dev shell, and Splora pass nixpkgs-rust.
# Do not pass the mail-host channel.
# This file does not select a compiler version and does not wrap cargo.
# The derivation name is surmount-rust.
{ pkgs }:
let
  compilers = [
    pkgs.rustc
    pkgs.cargo
    pkgs.clippy
    pkgs.rustfmt
  ];
in
pkgs.symlinkJoin {
  name = "surmount-rust";
  paths = compilers;
  passthru = {
    paths = compilers;
    rustc = pkgs.rustc;
    cargo = pkgs.cargo;
  };
}
