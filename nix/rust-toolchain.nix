# Crane / `nix develop` rustc. Pass nixpkgs-rust, not the mail-host channel.
# nixos-26.05 rustc is still 1.95.0. nixpkgs-rust is pinned to
# 9ecc0a672e9ef2ca4eec8e87dbee653607a7d332. That commit sets
# rustcVersion = "1.99.1" in pkgs/development/compilers/rust/1_99.nix.
# It is not merged to nixpkgs master. Master HEAD
# 27347cc2965498fa03393739031c9ef8ecd72c0a still has rustcVersion
# "1.99.0" in that file. Not beta 1.100. Prior lock
# c46e6c9a80d68a8b92e75a2d2cb078a8f6d8dd81 was rustc 1.99.0.
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
