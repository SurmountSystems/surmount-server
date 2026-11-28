# Re-call the locked Splora flake and point its crane toolchain at
# nix/rust-toolchain.nix. The compilers are pkgsRust.rustc and
# pkgsRust.cargo (the nixpkgs-rust package set the caller passes).
#
# The locked flake assigns rustToolchain from pkgs.rust-bin and then
# calls override. This file rewrites that assignment to pkgs.surmountRust
# before the flake is imported. It does not wrap cargo.
#
# The rewritten flake.nix must sit beside the rest of the Splora tree so
# relative paths in that flake still resolve. The copy is a local
# fixed-output-free derivation. Evaluating splora or splora-liquid
# imports it. Evaluating retarget or toolchain does not.
{
  lib,
  sploraInput,
  system,
  pkgsRust,
}:
let
  marker = "rustToolchain = pkgs.rust-bin";

  indexOfSemi =
    text:
    let
      len = builtins.stringLength text;
      go =
        i: depth:
        if i >= len then
          throw "nix/splora-toolchain.nix: rustToolchain assignment did not end"
        else
          let
            c = builtins.substring i 1 text;
          in
          if c == "{" then
            go (i + 1) (depth + 1)
          else if c == "}" then
            if depth == 0 then
              throw "nix/splora-toolchain.nix: rustToolchain assignment closed a brace it did not open"
            else
              go (i + 1) (depth - 1)
          else if c == ";" && depth == 0 then
            i
          else
            go (i + 1) depth;
    in
    go 0 0;

  retarget =
    text:
    let
      parts = lib.splitString marker text;
      count = builtins.length parts;
    in
    if count != 2 then
      throw "nix/splora-toolchain.nix: expected one rustToolchain assignment from pkgs.rust-bin, found ${toString (count - 1)}"
    else
      let
        prefix = builtins.elemAt parts 0;
        suffix = builtins.elemAt parts 1;
        end = indexOfSemi suffix;
        rest = builtins.substring (end + 1) (builtins.stringLength suffix - end - 1) suffix;
        patched = prefix + "rustToolchain = pkgs.surmountRust;" + rest;
      in
      if lib.hasInfix marker patched then
        throw "nix/splora-toolchain.nix: retarget left a pkgs.rust-bin toolchain assignment"
      else if !(lib.hasInfix "pkgs.surmountRust" patched) then
        throw "nix/splora-toolchain.nix: retarget did not point rustToolchain at pkgs.surmountRust"
      else
        patched;

  toolchain = import ./rust-toolchain.nix { pkgs = pkgsRust; };

  patchedText = retarget (builtins.readFile (sploraInput + "/flake.nix"));

  patchedSrc =
    pkgsRust.runCommand "splora-toolchain-src"
      {
        preferLocalBuild = true;
        allowSubstitutes = false;
        sploraSrc = sploraInput;
        flake = builtins.toFile "splora-flake.nix" patchedText;
      }
      ''
        set -eu
        mkdir -p "$out"
        cp -a "$sploraSrc"/. "$out"/
        cp "$flake" "$out/flake.nix"
      '';

  flake = import (patchedSrc + "/flake.nix");

  outputs = flake.outputs {
    self = sploraInput;
    nixpkgs = sploraInput.inputs.nixpkgs;
    rust-overlay = {
      overlays.default = _final: prev: {
        surmountRust = toolchain;
        rustc = pkgsRust.rustc;
        cargo = pkgsRust.cargo;
        rustPlatform = prev.makeRustPlatform {
          inherit (pkgsRust) rustc cargo;
        };
      };
    };
    crane = sploraInput.inputs.crane;
    asset-registry = sploraInput.inputs.asset-registry;
  };

  built = outputs.packages.${system} or (throw "splora flake outputs have no packages.${system}");
in
{
  inherit toolchain retarget;
  splora = built.splora or (throw "splora flake outputs have no packages.${system}.splora");
  splora-liquid =
    built.splora-liquid or (throw "splora flake outputs have no packages.${system}.splora-liquid");
}
