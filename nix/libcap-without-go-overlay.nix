# C libcap stays. setcap, getcap, and capsh stay. Those are the progs/
# directory, and the shared library is the libcap/ directory. Neither
# one is behind withGo.
#
# nixpkgs pkgs/by-name/li/libcap/package.nix defaults withGo on when a
# Go compiler is available (package.nix lines 11-12). It then lists go
# in nativeBuildInputs (lines 53-55) and appends GOLANG=yes (lines
# 66-72). The realized libcap 2.77 Makefile, lines 16-21, runs
# make -C go only inside ifeq ($(GOLANG),yes). make -C libcap and
# make -C progs sit outside that test. The 2.78 source tarball was not
# on disk. The instantiated libcap-2.78 derivation passes the same
# GOLANG=yes flag, so GOLANG=no is the skip.
#
# captree is built from go/ (go/Makefile line 19). The mail host does
# not install that program. This file does not add a replacement.
# htop is not added.
final: prev: {
  libcap = (prev.libcap.override { withGo = false; }).overrideAttrs (old: {
    nativeBuildInputs = [ ];
    makeFlags =
      (builtins.filter (
        flag:
        !(prev.lib.hasPrefix "GOLANG=" flag)
        && !(prev.lib.hasPrefix "GOCACHE=" flag)
        && !(prev.lib.hasPrefix "GOFLAGS=" flag)
        && !(prev.lib.hasPrefix "GOARCH=" flag)
        && !(prev.lib.hasPrefix "GOOS=" flag)
      ) (old.makeFlags or [ ]))
      ++ [ "GOLANG=no" ];
  });
}
