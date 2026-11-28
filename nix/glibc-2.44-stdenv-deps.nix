# Rebuild GNU C Library 2.44 with this package set's libidn2,
# libunistring, and linux-headers.
#
# nix/glibc-2.44-overlay.nix selects the 2.44 expression from Splora's
# locked nixpkgs (2786a2a72088ecf32aea5b3ec52099e6a7cff78f). Using that
# nixpkgs' finished glibc derivation also keeps its libidn2 2.3.8,
# libunistring 1.4.2, and linux-headers 7.3. The mail-host stdenv-linux
# allowedRequisites names the bootstrap copies from nixos-26.05
# (linux-headers 6.18.7, plus this set's libidn2 and libunistring).
# Those Splora store paths are a second package set. They are not the
# same paths with a hole in the allow-list.
#
# This file imports that 2.44 expression and builds it here. libidn2
# brings this set's libunistring. linux-headers is this set's
# linuxHeaders. The source stays glibc 2.44, which provides
# GLIBC_2.43 and GLIBC_2.44. The allow-list is not cleared.
#
# python3Minimal stays for configure.ac (python3 or python is a
# critical program) and for locale/gen-translit.py. It is not the
# gen-as-const interpreter. Makerules calls surmount-glibc-gen-as-const.
# This file does not assign python3, python3Minimal, or pythonPackages.
nixpkgs244: final: prev:
let
  lib = prev.lib;
  glibcDir = nixpkgs244.outPath + "/pkgs/development/libraries/glibc";
  commonFile = glibcDir + "/common.nix";
  versionCaptured = builtins.filter builtins.isList (
    builtins.split "version = \"([0-9]+\\.[0-9]+)\";" (builtins.readFile commonFile)
  );
  version = if versionCaptured == [ ] then "" else builtins.head (builtins.head versionCaptured);
  hostArch = prev.stdenv.hostPlatform.gcc.arch or null;
  upstreamPkgs = import prev.path {
    localSystem = {
      system = prev.stdenv.hostPlatform.system;
    }
    // lib.optionalAttrs (hostArch != null && hostArch != "") { gcc.arch = hostArch; };
  };
  glibcPython = upstreamPkgs.python3Minimal;
  genAsConst = import ./packages/glibc-gen-as-const.nix {
    system = prev.stdenv.hostPlatform.system;
  };
  # final.libidn2 is the bootstrap libidn2 after stdenvOverrides
  # (nuke-refs in the stage that builds glibc). That is the path
  # stdenv-linux allowedRequisites names. final.linuxHeaders is the
  # linux-headers that same allow-list names.
  deps = final // {
    python3Minimal = glibcPython;
  };
  callPackage = lib.callPackageWith (deps // { inherit callPackage; });
in
if builtins.match "2\\.(4[4-9]|[5-9][0-9]).*" version == null then
  throw "mail-host glibc must be a released 2.44 or newer, got ${version}"
else
  {
    # The 2.44 expression lists python3Minimal. Keep that input.
    # Append the Rust program and rewrite the three Makerules recipes
    # that used to run scripts/gen-as-const.py under $(PYTHON).
    glibc = (callPackage glibcDir {
      stdenv = final.gccStdenv;
      linuxHeaders = final.linuxHeaders;
    }).overrideAttrs (old: {
      nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ genAsConst ];
      postPatch = (old.postPatch or "") + genAsConst.makerulesEdit;
    });
  }
