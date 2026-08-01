# Stalwart CLI (schema-driven JMAP admin). Separate upstream repo from the server.
# Needed for `stalwart-cli apply` / import / day-to-day admin on 0.16+.
#
# Packaging mode: pinned upstream release binary FODs. Source builds need a
# newer rustc than nixos-25.05's 1.86 (unsigned_is_multiple_of, etc.).
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
  version = "1.0.12";

  sources = {
    x86_64-linux = {
      url = "https://github.com/stalwartlabs/cli/releases/download/v${version}/stalwart-cli-x86_64-unknown-linux-gnu.tar.xz";
      hash = "sha256-4rsFRQmqrDEfE/9PngnDjGBxld4ulzXPhM/G7kd2paI=";
      dir = "stalwart-cli-x86_64-unknown-linux-gnu";
    };
    aarch64-linux = {
      url = "https://github.com/stalwartlabs/cli/releases/download/v${version}/stalwart-cli-aarch64-unknown-linux-gnu.tar.xz";
      hash = "sha256-IRM0dLiAyWg2mZRkGXcoiW8X5yOpmWkOe/XQQjIyQNQ=";
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
