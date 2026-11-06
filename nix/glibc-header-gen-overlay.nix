# Mail-host glibc library headers without a Python interpreter.
# scripts/gen-as-const.py and locale/gen-translit.py stay in the
# upstream tarball. The two library recipes call nix/packages/glibc-header-gen.c
# instead. postPatch also drops python from configure's critical-program
# list. It does not add a python binary.
final: prev:
let
  lib = prev.lib;
  dropPythonNamed =
    deps:
    if !builtins.isList deps then
      deps
    else
      builtins.filter (
        dep:
        if !builtins.isAttrs dep then
          true
        else
          let
            name = dep.name or "";
            pname = dep.pname or "";
          in
          !(lib.hasInfix "python" name
            || lib.hasInfix "Python" name
            || lib.hasInfix "python" pname
            || lib.hasInfix "Python" pname)
      ) deps;
in
{
  glibc = prev.glibc.overrideAttrs (old: {
    nativeBuildInputs = dropPythonNamed (old.nativeBuildInputs or [ ]);
    postPatch = (old.postPatch or "") + ''
      cp ${./packages/glibc-header-gen.c} glibc-header-gen.c
      sed -i 's|$(PYTHON) $< --cc=|$(GLIBC_HEADER_GEN) as-const --cc=|' Makerules
      sed -i 's|$(PYTHON) gen-translit.py |$(GLIBC_HEADER_GEN) translit |' locale/Makefile
      grep -F -q '$(GLIBC_HEADER_GEN) as-const' Makerules
      grep -F -q '$(GLIBC_HEADER_GEN) translit' locale/Makefile
      # Unpacked source root: stdenv already cd'd here. The build runs
      # configure, not autoconf. Only the python critical-program
      # assignment is a no-op. make, gawk, and bison stay required.
      for f in configure configure.ac; do
        test -f "$f"
        grep -F -q 'critic_missing="$critic_missing python"' "$f"
      done
      sed -i 's/critic_missing="[$]critic_missing python"/:/' configure configure.ac
      for f in configure configure.ac; do
        if grep -F -q 'critic_missing="$critic_missing python"' "$f"; then
          echo "glibc $f still requires python" >&2
          exit 1
        fi
      done
      grep -F -q 'critic_missing="$critic_missing make"' configure
      grep -F -q 'critic_missing="$critic_missing gawk"' configure
      grep -F -q 'critic_missing="$critic_missing bison"' configure
    '';
    postConfigure = (old.postConfigure or "") + ''
      "$CC" -O2 -o glibc-header-gen ../glibc-header-gen.c
      export GLIBC_HEADER_GEN="$(pwd)/glibc-header-gen"
    '';
  });
}
