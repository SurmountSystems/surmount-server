# nixComponents_git 2.36pre20261109 (rev 16fa212a) has no meson
# option that skips clang-tidy. A per-component build is not a
# subproject, so the shared support file enters that directory and
# looks up python3. This overlay appends one patch before the pins
# overlay reads nix-cli. It does not select the 2.36.0 binary
# tarball. It does not add python3, python, or python3Minimal.
_final: prev: {
  nixVersions = prev.nixVersions // {
    nixComponents_git = prev.nixVersions.nixComponents_git.appendPatches [
      ./patches/nix-skip-clang-tidy.patch
    ];
  };
}
