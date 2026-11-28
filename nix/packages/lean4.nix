# Pinned Lean toolchain for the nixbuilder login profile.
# nixos-26.05 pkgs.lean4 is 4.29.1. Operator directed Lean 4.35 on
# 2026-11-04. This file is the official v4.35.0 linux x86_64 release,
# not a floating nixpkgs unstable fetch and not an elan toolchain.
#
# Asset: lean-4.35.0-linux.tar.zst
# https://github.com/leanprover/lean4/releases/tag/v4.35.0
# (published 2026-10-12T18:13:12Z, commit eb6cafb).
#
# Binaries keep their $ORIGIN runpaths into lib/ and lib/lean.
# Only bin/ interpreters are retargeted, because the release uses
# /lib64/ld-linux-x86-64.so.2, which NixOS does not provide.
# lib/ stays as upstream shipped it (leanc --sysroot).

{
  lib,
  stdenv,
  fetchurl,
  patchelf,
  zstd,
}:

let
  version = "4.35.0";
in
stdenv.mkDerivation {
  pname = "lean4";
  inherit version;

  src =
    if stdenv.hostPlatform.system != "x86_64-linux" then
      throw "lean4 ${version}: official linux x86_64 tarball only (got ${stdenv.hostPlatform.system})"
    else
      fetchurl {
        url = "https://github.com/leanprover/lean4/releases/download/v${version}/lean-${version}-linux.tar.zst";
        hash = "sha256-colZekRUMkbJInuMrlAXZ+CVzhf21Ei5X8So3Jcmj20=";
      };

  nativeBuildInputs = [
    patchelf
    zstd
  ];

  sourceRoot = "lean-${version}-linux";

  unpackPhase = ''
    runHook preUnpack
    tar -I zstd -xf "$src"
    runHook postUnpack
  '';

  dontConfigure = true;
  dontBuild = true;
  # Do not strip, and do not let fixup rewrite $ORIGIN runpaths.
  dontStrip = true;
  dontPatchELF = true;

  installPhase = ''
    runHook preInstall
    mkdir -p "$out"
    cp -a bin include lib share src LICENSE LICENSES "$out/"
    dyn="$(cat "${stdenv.cc}/nix-support/dynamic-linker")"
    find "$out/bin" -type f -print0 | while IFS= read -r -d "" f; do
      if patchelf --print-interpreter "$f" >/dev/null 2>&1; then
        patchelf --set-interpreter "$dyn" "$f"
      fi
    done
    test -x "$out/bin/lean"
    test -x "$out/bin/lake"
    test -x "$out/bin/leanc"
    test -x "$out/bin/leanmake"
    test -f "$out/share/lean/lean.mk"
    runHook postInstall
  '';

  meta = {
    description = "Lean 4.35.0 official linux x86_64 release (not nixpkgs lean4)";
    homepage = "https://github.com/leanprover/lean4";
    changelog = "https://github.com/leanprover/lean4/releases/tag/v${version}";
    license = lib.licenses.asl20;
    mainProgram = "lean";
    platforms = [ "x86_64-linux" ];
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
}
