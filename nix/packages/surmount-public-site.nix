# Static public site for apex/www (SurmountSystems/site).
# Call as: pkgs.callPackage ./surmount-public-site.nix { src = surmount-site; }
# src is the flake input (github:SurmountSystems/site, flake = false).
# No NPM. No build. HTML/CSS/JS/SVG/fonts only.

{
  lib,
  stdenvNoCC,
  src,
}:

stdenvNoCC.mkDerivation {
  pname = "surmount-public-site";
  version = "1";

  inherit src;

  dontConfigure = true;
  dontBuild = true;
  dontFixup = true;

  installPhase = ''
    runHook preInstall
    mkdir -p "$out"
    # Public HTML/CSS/JS/SVG/fonts only. Skip flake, Lean, nav-ssg, docs, git.
    find . -maxdepth 1 -type f \( \
      -name '*.html' -o -name '*.css' -o -name '*.js' \
      -o -name '*.svg' -o -name '*.ico' \
    \) -exec cp -a {} "$out/" \;
    if [ -d fonts ]; then
      cp -a fonts "$out/"
    fi
    runHook postInstall
  '';

  meta = {
    description = "Surmount Systems public static site for apex and www";
    homepage = "https://github.com/SurmountSystems/site";
    platforms = lib.platforms.all;
  };
}
