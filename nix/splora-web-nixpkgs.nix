# Package set for packages.splora-web.
#
# This is not nixosConfigurations.mail-vps.pkgs. The web trunk does not
# take the mail-host pins, glibc, or Python-refusal overlays. Splora
# rev 1a50824 does not put Sass on the release web. crane
# buildTrunkPackage still appends dart-sass; nix/packages/splora-web.nix
# removes that input. This import does not rewrite buf and does not
# overlay libcap. mail-vps.pkgs.libcap still uses
# nix/libcap-without-go-overlay.nix.
{ nixpkgs, system }:
import nixpkgs { inherit system; }
