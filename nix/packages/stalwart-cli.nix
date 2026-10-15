# Stalwart CLI (schema-driven JMAP admin). Separate upstream repo from the server.
# Needed for `stalwart-cli apply` / directory / day-to-day admin on 0.16+.
# Maildir import is Vandelay, not this CLI (1.0.x has no import subcommand).
#
# Packaging mode: pinned upstream release binary FODs. Source builds need a
# newer rustc than older host channels' defaults (unsigned_is_multiple_of, etc.).
#
# Bump: set version, prefetch each arch tar.xz, paste hashes.

{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  openssl,
  zlib,
}:

let
  version = "1.0.13";

  sources = {
    x86_64-linux = {
      url = "https://github.com/stalwartlabs/cli/releases/download/v${version}/stalwart-cli-x86_64-unknown-linux-gnu.tar.xz";
      hash = "sha256-G4UJt2ft0aF2k+CStRjEFhDsS3JPTmLEYb0ozUDGafc=";
      dir = "stalwart-cli-x86_64-unknown-linux-gnu";
    };
    aarch64-linux = {
      url = "https://github.com/stalwartlabs/cli/releases/download/v${version}/stalwart-cli-aarch64-unknown-linux-gnu.tar.xz";
      hash = "sha256-ZfS25SjYQ4iVP8mNlGM53YJG+wO/TKqmTh2bN49S2JI=";
      dir = "stalwart-cli-aarch64-unknown-linux-gnu";
    };
  };

  spec =
    sources.${stdenv.hostPlatform.system}
      or (throw "stalwart-cli ${version}: no prebuilt binary for ${stdenv.hostPlatform.system}");
in
stdenv.mkDerivation {
  pname = "stalwart-cli";
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
    install -Dm755 stalwart-cli $out/bin/stalwart-cli
    runHook postInstall
  '';

  meta = {
    description = "Stalwart command-line interface (JMAP management)";
    homepage = "https://github.com/stalwartlabs/cli";
    changelog = "https://github.com/stalwartlabs/cli/blob/v${version}/CHANGELOG.md";
    license = lib.licenses.agpl3Only;
    mainProgram = "stalwart-cli";
    platforms = lib.attrNames sources;
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
}
