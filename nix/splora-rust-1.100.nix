# Splora's mkPkgs takes only pkgs. It hardcodes
# pkgs.rust-bin.stable."1.99.1".default and does not take toolchain,
# rustc, or cargo. The sibling flake is not edited.
# Locked rust-overlay (0d0575988e4bb9ebc4cf1afb1cec1ceac0b5914e) has
# manifests/stable through 1.99.1.nix and no 1.100.0 file. Do not invent
# rust-bin.stable."1.100.0". Do not select a compiler newer than 1.100.0.
# Re-call the flake outputs with an overlay that puts the nixpkgs-rust
# 1.100.0 crane join (nix/rust-toolchain.nix: rustc and cargo, same
# compilers as mkArtiRustPlatform) on the attribute Splora reads.
{ lib, sploraInput, system, pkgsRust }:
let
  toolchain = import ./rust-toolchain.nix { pkgs = pkgsRust; };
  toolchainOverlay = _final: prev: {
    rust-bin.stable."1.99.1".default = toolchain;
    rustc = pkgsRust.rustc;
    cargo = pkgsRust.cargo;
    rustPlatform = prev.makeRustPlatform {
      inherit (pkgsRust) rustc cargo;
    };
  };
  flake = import (sploraInput + "/flake.nix");
  outputs = flake.outputs {
    self = sploraInput;
    nixpkgs = sploraInput.inputs.nixpkgs;
    rust-overlay = {
      overlays.default = toolchainOverlay;
    };
    crane = sploraInput.inputs.crane;
    asset-registry = sploraInput.inputs.asset-registry;
  };
  built = outputs.packages.${system} or (throw "splora flake outputs have no packages.${system}");
in
assert lib.assertMsg (pkgsRust.rustc.version == "1.100.0") ''
  nix/splora-rust-1.100.nix: nixpkgs-rust rustc ${pkgsRust.rustc.version} is not 1.100.0.
'';
assert lib.assertMsg (pkgsRust.cargo.version == "1.100.0") ''
  nix/splora-rust-1.100.nix: nixpkgs-rust cargo ${pkgsRust.cargo.version} is not 1.100.0.
'';
assert lib.assertMsg (lib.hasInfix "1.100.0" toolchain.name) ''
  nix/splora-rust-1.100.nix: toolchain "${toolchain.name}" is not 1.100.0.
'';
assert lib.assertMsg (!lib.hasInfix "1.99.1" toolchain.name) ''
  nix/splora-rust-1.100.nix: refusing to select "${toolchain.name}".
'';
{
  splora = built.splora or (throw "splora flake outputs have no packages.${system}.splora");
  splora-liquid =
    built.splora-liquid or (throw "splora flake outputs have no packages.${system}.splora-liquid");
}
