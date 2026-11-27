# pcsclite 2.5.3 ships etc/pcscd.service.in without @requires_polkit@.
# The unit file uses @polkit_dependencies@. A postPatch that runs
# substituteInPlace --replace-fail on @requires_polkit@ fails in
# patchPhase, because those bytes are not in the file.
#
# noHaskellOverlay still appends that substitute. This file is merged
# after that attribute, so this pcsclite replaces it. postPatch is the
# earlier postPatch. If the failing substitute text is already in
# postPatch, it is removed. Nothing is written in its place. The
# upstream unit file stays as shipped. No polkit unit line is added.
# The nixpkgs meson flag -Dpolkit=false stays. pcsclite stays.
# libfido2 is not edited.
#
# mkdir of the doc output is the same directory the mail-host override
# already created. That is not a change to the service file.
final: prev: {
  pcsclite = prev.pcsclite.overrideAttrs (old: {
    postPatch =
      let
        prior = old.postPatch or "";
        stale = ''
          substituteInPlace etc/pcscd.service.in \
            --replace-fail '@requires_polkit@' ""
        '';
      in
      builtins.replaceStrings [ stale ] [ "" ] prior;
    postInstall = (old.postInstall or "") + ''
      mkdir -p "$doc"
    '';
  });
}
