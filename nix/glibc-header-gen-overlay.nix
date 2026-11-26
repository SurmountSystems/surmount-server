# Locale archive and the glibc configure interpreter. Top-level
# python, python3, python3Minimal, and pythonPackages still throw.
# This file does not assign those names.
# nix/glibc-2.44-overlay.nix runs after this overlay and replaces
# the glibc attribute with released glibc 2.44. This file still
# strips Python from glibcLocales.
# scripts/gen-as-const.py is not invoked. Makerules calls
# surmount-glibc-gen-as-const (nix/packages/glibc-gen-as-const.nix).
# The .py file stays in the glibc source tree.
# configure.ac still requires python3 or python on PATH, and
# locale/gen-translit.py still runs during before-compile. Those are
# why glibcPython stays. This import is not final.python3 and not
# final.python. It does not replace the configure critical-program
# check with a colon.
# glibcLocales does not run configure, ./configure, or the glibc
# configure script. Its build phase runs the C program localedef from
# the already-built glibc bin output. localedef --prefix writes
# locale-archive under $out plus the absolute localedir compiled into
# that binary. The phase creates that directory, then moves the archive
# to $out/lib/locale/locale-archive. It reads $PWD/localedata/SUPPORTED
# from the unpacked glibc source. It does not read
# ../localedata/SUPPORTED.
final: prev:
let
  lib = prev.lib;
  # prev.python3 here is already the throw (noHaskell runs first).
  # A second nixpkgs import has no throw overlay, so its python3Minimal
  # is the interpreter glibc's own build used upstream. final.python3
  # stays the throw.
  hostArch = prev.stdenv.hostPlatform.gcc.arch or null;
  upstreamPkgs = import prev.path {
    localSystem =
      { system = prev.stdenv.hostPlatform.system; }
      // lib.optionalAttrs (hostArch != null && hostArch != "") { gcc.arch = hostArch; };
  };
  glibcPython = upstreamPkgs.python3Minimal;
  # Separate nixpkgs-rust import (rustc 1.100.0). Not the glibc being built.
  genAsConst = import ./packages/glibc-gen-as-const.nix {
    system = prev.stdenv.hostPlatform.system;
  };
  # glibc common.nix lists python3Minimal in nativeBuildInputs.
  # The mail-host overlay sets that attribute to a throw. isAttrs
  # forces the throw and aborts eval. tryEval drops that element.
  # It does not bind python3, python3Minimal, or pythonPackages.
  # Asking for those attributes still throws. A real derivation
  # is still dropped when its name contains python.
  dropPythonNamed =
    deps:
    if !builtins.isList deps then
      deps
    else
      builtins.filter (
        dep:
        let
          tried = builtins.tryEval dep;
        in
        if !tried.success then
          false
        else if !builtins.isAttrs tried.value then
          true
        else
          let
            name = tried.value.name or "";
            pname = tried.value.pname or "";
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
  # dropPythonNamed removes the throw and any other interpreter-named
  # input the scrub already rewrote. glibcPython is then the only
  # interpreter on this derivation. Configure is not patched. The
  # critical-program check stays in the upstream script.
  # makerulesEdit stops the three $(PYTHON) recipes that ran
  # scripts/gen-as-const.py. It does not hide python3 behind a colon.
  glibcLibraryBuild = old: {
    nativeBuildInputs = (dropPythonNamed (old.nativeBuildInputs or [ ])) ++ [
      glibcPython
      genAsConst
    ];
    postPatch = (old.postPatch or "") + genAsConst.makerulesEdit;
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
  glibc = prev.glibc.overrideAttrs glibcLibraryBuild;
  # override.__functionArgs is name -> has-default, not the applied
  # callPackage values. Those booleans would replace locales and glibc.
  # {} keeps the arguments already applied. .override { allLocales, locales }
  # still runs localesFromLocaledef after the original function.
  glibcLocales =
    let
      functionArgs = prev.glibcLocales.override.__functionArgs or { };
      appliedArgs =
        if builtins.all builtins.isBool (builtins.attrValues functionArgs) then { } else functionArgs;
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
