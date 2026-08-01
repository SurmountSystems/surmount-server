# Surmount-controlled Stalwart Mail and Collaboration Server package.
#
# Why not pkgs.stalwart-mail from nixos-25.05?
#   That channel packages 0.11.8 (scaffold accident). Upstream latest stable is
#   far ahead. Greenfield Surmount pins the current engine here.
#
# Why not only pkgs.stalwart / stalwart_0_16 from unstable?
#   Unstable lagged at 0.16.14 when checked, and services.stalwart still targets
#   the 0.15 TOML world. We own the binary pin and a Surmount module for 0.16
#   config.json (see modules/stalwart-service.nix).
#
# Packaging mode: pinned upstream *release binary* FODs (default).
#   Cargo source builds (rustPlatform) are preferred when cargo vendor works,
#   but crates.io returned HTTP 403 from this environment's fetch-cargo-vendor
#   (2026-07-30). Release tarballs are hermetic fixed-output hashes and match
#   the published tag. Revisit source builds when vendor works again (see
#   nix/packages/stalwart-mail-from-source.nix.disabled notes in the join).
#
# Bump procedure:
#   1. Set version = "X.Y.Z" to match GitHub tag vX.Y.Z on stalwartlabs/stalwart.
#   2. Prefetch each arch:
#        nix store prefetch-file \
#          https://github.com/stalwartlabs/stalwart/releases/download/vX.Y.Z/stalwart-x86_64-unknown-linux-gnu.tar.gz
#        nix store prefetch-file \
#          https://github.com/stalwartlabs/stalwart/releases/download/vX.Y.Z/stalwart-aarch64-unknown-linux-gnu.tar.gz
#   3. Paste hashes into sources below.
#   4. Bump webui / spam-filter FODs if upstream release notes require it.
#   5. Re-read UPGRADING notes in the tag for config surface changes.

{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  openssl,
  zlib,
  callPackage,
}:

let
  version = "0.16.15";

  # Release asset hashes (gnu libc). musl variants exist upstream if needed.
  sources = {
    x86_64-linux = fetchurl {
      url = "https://github.com/stalwartlabs/stalwart/releases/download/v${version}/stalwart-x86_64-unknown-linux-gnu.tar.gz";
      hash = "sha256-byPGIX8PlxC6/eiJt++8NJx8+mE8SxS4s2UkV5k3NnY=";
    };
    aarch64-linux = fetchurl {
      url = "https://github.com/stalwartlabs/stalwart/releases/download/v${version}/stalwart-aarch64-unknown-linux-gnu.tar.gz";
      hash = "sha256-SArFIesOCMxntsI3LN9QT50Jd6uUba0ILdrMMDdDfiw=";
    };
  };

  src =
    sources.${stdenv.hostPlatform.system}
      or (throw "stalwart-mail ${version}: no prebuilt binary for ${stdenv.hostPlatform.system}");
in
stdenv.mkDerivation (finalAttrs: {
  pname = "stalwart-mail";
  inherit version src;

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
  ];

  buildInputs = [
    openssl
    zlib
    stdenv.cc.cc.lib
  ];

  # Tarball contains a single binary named "stalwart".
  sourceRoot = ".";
  unpackPhase = ''
    runHook preUnpack
    tar -xzf $src
    runHook postUnpack
  '';

  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin $out/etc/stalwart
    install -Dm755 stalwart $out/bin/stalwart
    # Compatibility symlink for older docs / habits.
    ln -s stalwart $out/bin/stalwart-mail
    runHook postInstall
  '';

  # Keep OpenSSL discoverable if the binary is dynamically linked against it.
  autoPatchelfIgnoreMissingDeps = [
    # Some builds are mostly static; ignore soft missing libs if any.
  ];

  passthru = {
    # FOD companions (file:// pins for hermetic hosts).
    webui = callPackage ./stalwart-webui.nix { };
    spam-filter = callPackage ./stalwart-spam-filter.nix { };
    # Alias used by older nixpkgs modules / docs.
    webadmin = callPackage ./stalwart-webui.nix { };
    # How this derivation was produced (join / ops introspection).
    packagingMode = "release-binary-fod";
    updateScript = null;
  };

  meta = {
    description = "Secure, modern, all-in-one mail and collaboration server (Surmount pin, release binary)";
    homepage = "https://github.com/stalwartlabs/stalwart";
    changelog = "https://github.com/stalwartlabs/stalwart/blob/v${version}/CHANGELOG.md";
    license = lib.licenses.agpl3Only;
    mainProgram = "stalwart";
    platforms = lib.attrNames sources;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
})
