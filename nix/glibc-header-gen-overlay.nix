# Mail-host glibc without a Python interpreter.
# The library derivation runs glibc configure. configure lists python
# as a critical program (configure: critic_missing="$critic_missing python"
# after "checking for python3" / "checking for python").
# scripts/gen-as-const.py and locale/gen-translit.py stay in the
# upstream tarball. The library recipe calls nix/packages/glibc-header-gen.c
# instead. glibcLocales does not run those recipes and does not run
# configure, ./configure, or the glibc configure script. Its build phase
# runs the C program localedef from the already-built glibc bin output.
# localedef --prefix writes locale-archive under $out plus the absolute
# localedir compiled into that binary. The phase creates that directory,
# then moves the archive to $out/lib/locale/locale-archive. It reads
# $PWD/localedata/SUPPORTED from the unpacked glibc source. It does not
# read ../localedata/SUPPORTED. The library postPatch still
# drops python from configure's critical-program list, so a later library
# recipe cannot call python either. It does not add a python binary.
# glibcLocales does not use that sed.
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
          !(
            lib.hasInfix "python" name
            || lib.hasInfix "Python" name
            || lib.hasInfix "python" pname
            || lib.hasInfix "Python" pname
          )
      ) deps;
  # Library only. glibcLocales is a separate derivation: locales.nix
  # calls callPackage on common.nix, so this overrideAttrs does not
  # reach it. glibcLocales skips configure and runs localedef.
  # callPackage's overrideAttrs keeps .override, which NixOS uses as
  # pkgs.glibcLocales.override { allLocales, locales }.
  withoutPythonInterpreter = old: {
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
  };
  # Do not run glibc configure. localedef is the C binary from the
  # built glibc bin output. The locale archive is the build product.
  # localedef --prefix writes it under $out plus the absolute localedir
  # compiled into that binary. The phase creates that directory, then
  # moves locale-archive to $out/lib/locale/locale-archive.
  # Upstream preBuild appends to ../localedata/SUPPORTED from an
  # out-of-tree build/ directory. This phase never creates build/.
  # ":" replaces that hook. The phase reads $PWD/localedata/SUPPORTED
  # from the unpacked source instead.
  localesFromLocaledef = old: {
    dontConfigure = true;
    configurePhase = "true";
    configureScript = "true";
    preConfigure = "";
    postConfigure = "";
    preBuild = ":";
    nativeBuildInputs = dropPythonNamed (old.nativeBuildInputs or [ ]);
    glibcLocalesList =
      if (old.allLocales or true) || !(builtins.isList (old.locales or null)) then
        ""
      else
        lib.concatStringsSep " " old.locales;
    buildPhase = ''
      if [ -n "''${sourceRoot:-}" ]; then
        if [ -d "$sourceRoot" ]; then
          cd "$sourceRoot"
        elif [ -d "$NIX_BUILD_TOP/$sourceRoot" ]; then
          cd "$NIX_BUILD_TOP/$sourceRoot"
        fi
      fi
      supported="$PWD/localedata/SUPPORTED"
      test -f "$supported"
      mkdir -p "$out/lib/locale"
      localedef="${lib.getBin final.glibc}/bin/localedef"
      test -x "$localedef"
      unset LOCPATH
      export I18NPATH="$PWD/localedata"
      export LC_ALL=C
      export LANG=C
      # configure is not run for glibcLocales. This phase does not execute
      # configure, ./configure, or the glibc configure script.
      # --prefix "$out" prepends $out to the absolute localedir compiled
      # into this localedef. Read that directory from this binary. The
      # store hash changes on every glibc build, so it is not written here.
      compiled_localedir="$(
        grep -a -o -E '/nix/store/[0-9a-z]{32}-[^[:space:]/]+/lib/locale' "$localedef" \
          | sort -u || true
      )"
      compiled_count="$(printf '%s\n' "$compiled_localedir" | grep -c '^/nix/store/.*/lib/locale$' || true)"
      if [ "$compiled_count" -ne 1 ]; then
        echo "glibcLocales: expected one compiled-in localedir from $localedef, found $compiled_count" >&2
        exit 1
      fi
      help_localedir="$("$localedef" --help 2>&1 | sed -n '/locale path/{n;s/:.*//;s/^[[:space:]]*//;s/[[:space:]]*$//;p;q;}' || true)"
      if [ "$help_localedir" != "$compiled_localedir" ]; then
        echo "glibcLocales: localedef --help localedir does not match the binary" >&2
        exit 1
      fi
      mkdir -p "$out$compiled_localedir"
      build_one() {
        entry="$1"
        case "$entry" in
          */*) ;;
          *)
            echo "glibcLocales: skip $entry" >&2
            return 0
            ;;
        esac
        name="''${entry%%/*}"
        charset="''${entry#*/}"
        base="''${name%%.*}"
        if [ "$name" = "''${name#*@}" ]; then
          input_name="$base"
        else
          input_name="$base@''${name#*@}"
        fi
        input="$PWD/localedata/locales/$input_name"
        charmap="$PWD/localedata/charmaps/$charset"
        if [ ! -f "$input" ] || [ ! -f "$charmap" ]; then
          echo "glibcLocales: skip $entry" >&2
          return 0
        fi
        if [ -f "$PWD/intl/locale.alias" ]; then
          "$localedef" --force \
            --alias-file="$PWD/intl/locale.alias" \
            -i "$input" \
            -f "$charmap" \
            --prefix "$out" \
            "$name"
        else
          "$localedef" --force \
            -i "$input" \
            -f "$charmap" \
            --prefix "$out" \
            "$name"
        fi
      }
      while IFS= read -r line || [ -n "$line" ]; do
        case "$line" in
          ""|\#*) continue ;;
        esac
        entry="''${line%%\\}"
        entry="''${entry%"''${entry##*[![:space:]]}"}"
        entry="''${entry#"''${entry%%[![:space:]]*}"}"
        entry="''${entry%%[[:space:]]*}"
        [ -z "$entry" ] && continue
        if [ -n "''${glibcLocalesList:-}" ]; then
          case " $glibcLocalesList " in
            *" $entry "*) ;;
            *) continue ;;
          esac
        fi
        build_one "$entry"
      done < "$supported"
      mkdir -p "$out/lib/locale"
      if [ -f "$out$compiled_localedir/locale-archive" ]; then
        mv -f "$out$compiled_localedir/locale-archive" "$out/lib/locale/locale-archive"
      fi
      test -s "$out/lib/locale/locale-archive"
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      test -s "$out/lib/locale/locale-archive"
      runHook postInstall
    '';
  };
in
{
  glibc = prev.glibc.overrideAttrs withoutPythonInterpreter;
  # override.__functionArgs is name -> has-default, not the applied
  # callPackage values. Those booleans would replace locales and glibc.
  # {} keeps the arguments already applied. .override { allLocales, locales }
  # still runs localesFromLocaledef after the original function.
  glibcLocales =
    let
      functionArgs = prev.glibcLocales.override.__functionArgs or { };
      appliedArgs =
        if builtins.all builtins.isBool (builtins.attrValues functionArgs) then
          { }
        else
          functionArgs;
    in
    prev.lib.makeOverridable (
      attrs: (prev.glibcLocales.override attrs).overrideAttrs localesFromLocaledef
    ) appliedArgs;
  # Separate callPackage of locales.nix (allLocales = false), not
  # glibcLocales.override. fetchzip and libarchive take this attribute.
  # allLocales is not a derivation attribute, so the wrapper passes it
  # into localesFromLocaledef. Do not add python.
  glibcLocalesUtf8 = prev.lib.makeOverridable (
    attrs:
    (prev.glibcLocalesUtf8.override attrs).overrideAttrs (
      old:
      localesFromLocaledef (
        old
        // {
          allLocales = attrs.allLocales or false;
          locales = attrs.locales or [ "en_US.UTF-8/UTF-8" ];
        }
      )
    )
  ) { };
}
