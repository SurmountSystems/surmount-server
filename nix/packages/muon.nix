# Upstream muon 0.7.0 (git tag 0.7.0, commit
# 74f8b8858f59e158bd12b417931d308b64105bed).
# meson.build sets meson_compat to 1.11. src/version.c.in appends
# .99, so the binary reports 1.11.99. nixos-26.05 ships 0.5.0, which
# reports 1.7.99. nix libutil requires meson >= 1.8.
# Docs and tests stay off. This file does not take an interpreter.
# The source hash is the nixpkgs fetchFromSourcehut hash for this tag.
{
  lib,
  stdenv,
  fetchFromSourcehut,
  pkgconf,
  curl,
  libarchive,
  zlib,
  samurai,
  embedSamurai ? true,
}:
stdenv.mkDerivation (finalAttrs: {
  pname = "muon" + lib.optionalString embedSamurai "-embedded-samurai";
  version = "0.7.0";

  src = fetchFromSourcehut {
    name = "muon-src";
    owner = "~lattis";
    repo = "muon";
    tag = finalAttrs.version;
    hash = "sha256-OJvduSTJSSVWMynDP2DnMMqEirQjsvTIGXNYatGHdd4=";
  };

  strictDeps = true;

  nativeBuildInputs = [
    pkgconf
  ] ++ lib.optionals (!embedSamurai) [ samurai ];

  buildInputs = [
    curl
    libarchive
    zlib
  ];

  enableParallelBuilding = true;

  buildPhase =
    let
      muonBool = lib.mesonBool;
      muonEnable = lib.mesonEnable;
      muonOption = lib.mesonOption;
      bootstrapFlags = lib.optionalString (!embedSamurai) "CFLAGS=\"$CFLAGS -DBOOTSTRAP_NO_SAMU\"";
      cmdlineForMuon = lib.concatStringsSep " " (
        [
          (muonOption "prefix" (placeholder "out"))
          (muonEnable "auto_features" true)
          (muonOption "buildtype" "plain")
          (muonOption "optimization" "plain")
          (muonOption "wrap_mode" "nodownload")
          (muonBool "static" stdenv.targetPlatform.isStatic)
          (muonEnable "man-pages" false)
          (muonEnable "meson-docs" false)
          (muonEnable "meson-tests" false)
          (muonEnable "libpkgconf" false)
          (muonEnable "samurai" embedSamurai)
          (muonEnable "tracy" false)
          (muonEnable "website" false)
          (muonEnable "native_backtrace" stdenv.targetPlatform.isGnu)
        ]
        ++ lib.optionals stdenv.targetPlatform.isGnu [
          (muonOption "c_args" "-D_GNU_SOURCE")
        ]
      );
      cmdlineForSamu = "-j$NIX_BUILD_CORES";
    in
    ''
      runHook preBuild

      ${bootstrapFlags} ./bootstrap.sh stage-1
      ./stage-1/muon-bootstrap setup ${cmdlineForMuon} stage-2
      ${lib.optionalString embedSamurai "./stage-1/muon-bootstrap"} samu ${cmdlineForSamu} -C stage-2

      runHook postBuild
    '';

  installPhase = ''
    runHook preInstall

    stage-2/muon -C stage-2 install

    runHook postInstall
  '';

  meta = {
    homepage = "https://muon.build";
    description = "Implementation of the meson build system in C99";
    license = lib.licenses.gpl3Only;
    platforms = lib.platforms.unix;
    mainProgram = "muon";
  };
})
