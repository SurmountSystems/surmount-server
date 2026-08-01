# Pinned spam-filter rules for Stalwart (FOD release assets).
# Avoids impure GitHub downloads at runtime when the operator points
# SpamSettings at these file:// paths via WebUI / stalwart-cli apply.
#
# Bump: update version + re-prefetch both hashes:
#   nix store prefetch-file https://github.com/stalwartlabs/spam-filter/releases/download/vX.Y.Z/spam-filter.toml
#   nix store prefetch-file https://github.com/stalwartlabs/spam-filter/releases/download/vX.Y.Z/spam-filter-rules.json.gz

{
  lib,
  stdenvNoCC,
  fetchurl,
}:

stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "stalwart-spam-filter";
  version = "3.0.0";

  srcToml = fetchurl {
    url = "https://github.com/stalwartlabs/spam-filter/releases/download/v${finalAttrs.version}/spam-filter.toml";
    hash = "sha256-SsA1KDJA5Xcq0eQiumipy8uYW43obcTIjbtgBhtRvQc=";
  };

  srcRules = fetchurl {
    url = "https://github.com/stalwartlabs/spam-filter/releases/download/v${finalAttrs.version}/spam-filter-rules.json.gz";
    hash = "sha256-xObN2h5oG2HtWWFuOJSp8rv/jJT8ugFCfDC1sLMP7vo=";
  };

  dontUnpack = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp ${finalAttrs.srcToml} $out/spam-filter.toml
    cp ${finalAttrs.srcRules} $out/spam-filter-rules.json.gz
    runHook postInstall
  '';

  meta = {
    description = "Pinned Stalwart spam-filter rules (release FODs)";
    homepage = "https://github.com/stalwartlabs/spam-filter";
    license = with lib.licenses; [
      mit
      asl20
    ];
  };
})
