# Pinned spam-filter rules for Stalwart (FOD release asset).
# Avoids impure GitHub downloads at runtime when the operator points
# SpamSettings at this file:// path via WebUI / stalwart-cli apply.
#
# Checked 2026-11-03: GitHub releases/latest is v3.0.4
# (published 2026-11-01T15:50:40Z). v3.0.2 still published spam-filter.toml.
# v3.0.3 and v3.0.4 publish only spam-filter-rules.json.gz.
# Do not invent a toml URL for a tag that does not ship one.
#
# Bump: update version + re-prefetch:
#   nix store prefetch-file https://github.com/stalwartlabs/spam-filter/releases/download/vX.Y.Z/spam-filter-rules.json.gz

{
  lib,
  stdenvNoCC,
  fetchurl,
}:

stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "stalwart-spam-filter";
  version = "3.0.4";

  src = fetchurl {
    url = "https://github.com/stalwartlabs/spam-filter/releases/download/v${finalAttrs.version}/spam-filter-rules.json.gz";
    hash = "sha256-YEzEOGsVdOlRhdN8ucGUrCDTYMyjhllgtu0O33BHVWI=";
  };

  dontUnpack = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp ${finalAttrs.src} $out/spam-filter-rules.json.gz
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
