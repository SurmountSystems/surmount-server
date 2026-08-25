# Vandelay: official Stalwart 0.16+ JMAP importer-exporter.
#
# stalwart-cli 1.0.x has no import/export of Maildir. MailPlus import uses
# this binary (Maildir++ -> SQLite archive -> JMAP export). See
# https://github.com/stalwartlabs/vandelay (accessed: 2026-08-13) and
# docs/MIGRATION.md.
#
# Packaging mode: pinned upstream release binary FODs (same pattern as
# stalwart-cli). Bump: set version, prefetch each arch tar.gz, paste hashes.

{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  openssl,
  zlib,
}:

let
  version = "1.0.7";

  sources = {
    x86_64-linux = {
      url = "https://github.com/stalwartlabs/vandelay/releases/download/v${version}/vandelay-x86_64-unknown-linux-gnu.tar.gz";
      hash = "sha256-6maiYWqAfYrqb1DBbg0xk/4P4kOAzkn6eEsNACnz1v4=";
      dir = "vandelay-x86_64-unknown-linux-gnu";
    };
    aarch64-linux = {
      url = "https://github.com/stalwartlabs/vandelay/releases/download/v${version}/vandelay-aarch64-unknown-linux-gnu.tar.gz";
      hash = "sha256-SvpCtDh94+dCDh6GyhmMW62UOwVHmxRNU6FomaFzY6A=";
      dir = "vandelay-aarch64-unknown-linux-gnu";
    };
  };

  spec =
    sources.${stdenv.hostPlatform.system}
      or (throw "vandelay ${version}: no prebuilt binary for ${stdenv.hostPlatform.system}");
in
stdenv.mkDerivation {
  pname = "vandelay";
  inherit version;

  src = fetchurl {
    inherit (spec) url hash;
  };

  nativeBuildInputs = [ autoPatchelfHook ];
  buildInputs = [
    openssl
    zlib
    stdenv.cc.cc.lib
  ];

  sourceRoot = spec.dir;

  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin
    install -Dm755 vandelay $out/bin/vandelay
    runHook postInstall
  '';

  meta = {
    description = "JMAP importer-exporter (Maildir++ / IMAP / JMAP)";
    homepage = "https://github.com/stalwartlabs/vandelay";
    changelog = "https://github.com/stalwartlabs/vandelay/blob/v${version}/CHANGELOG.md";
    license = lib.licenses.mit;
    mainProgram = "vandelay";
    platforms = lib.attrNames sources;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
}
