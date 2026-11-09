# Pinned Stalwart WebUI zip (FOD release asset).
# Stalwart 0.16+ ships the management UI as an Application resource_url.
# Defaults point at GitHub latest (impure). Point Application.resource_url
# at file://${this}/webui.zip via WebUI or stalwart-cli apply for hermetic hosts.
#
# Bump: update version + re-prefetch:
#   nix store prefetch-file https://github.com/stalwartlabs/webui/releases/download/vX.Y.Z/webui.zip

{
  lib,
  stdenvNoCC,
  fetchurl,
}:

stdenvNoCC.mkDerivation (finalAttrs: {
  pname = "stalwart-webui";
  # Checked 2026-11-03: GitHub releases/latest is still v2.0.4
  # (published 2026-11-02T11:19:17Z). Asset name is still webui.zip.
  version = "2.0.4";

  src = fetchurl {
    url = "https://github.com/stalwartlabs/webui/releases/download/v${finalAttrs.version}/webui.zip";
    hash = "sha256-cni+Zwh0Y+ARq/QBeKL/Bz0Bu/AuYq0qP/BENRQAIrA=";
  };

  dontUnpack = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp ${finalAttrs.src} $out/webui.zip
    # Alias for modules that still look for the old webadmin.zip name.
    ln -s webui.zip $out/webadmin.zip
    runHook postInstall
  '';

  meta = {
    description = "Pinned Stalwart WebUI assets (release FOD)";
    homepage = "https://github.com/stalwartlabs/webui";
    license = lib.licenses.agpl3Only;
  };
})
