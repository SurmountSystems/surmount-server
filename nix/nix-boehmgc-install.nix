# The upstream Nix 2.36.0 binary asks for libgc.so.1.
# pkgs/by-name/bo/boehmgc is Boehm GC (pname boehm-gc). The tarball
# has libgc.so.1 only as a symlink, so the install's find -type f
# misses it, and glibc does not provide it. Use this package set's
# Boehm GC on the rpath. Do not build Nix from source.
# Later SONAMEs in that tarball are the same kind of symlink
# (libmimalloc.so.3, libstdc++.so.6). Copy those by dereferencing.
final: nixPkg:
nixPkg.overrideAttrs (
  old:
  let
    glibcLib = "${final.glibc}/lib";
    gcLib = "${final.boehmgc}/lib";
    phase = old.installPhase or "";
    fromRpath = ''rpath="${glibcLib}:$out/lib"'';
    toRpath = ''rpath="${glibcLib}:${gcLib}:$out/lib"'';
    fromCheck = ''if [ -e "${glibcLib}/$base" ]; then'';
    toCheck = ''if [ -e "${gcLib}/$base" ] || [ -e "${glibcLib}/$base" ]; then'';
    fromFind = ''find . -type f -name "$base"'';
    toFind = ''find . \( -type f -o -type l \) -name "$base"'';
    fromCopy = ''cp -a "$src" "$out/lib/$base"'';
    toCopy = ''cp -L "$src" "$out/lib/$base"'';
    # lib.hasInfix uses builtins.match, which rejects a pattern that
    # refers to a store path. replaceStrings does not.
    contains = needle: haystack:
      builtins.replaceStrings [ needle ] [ "" ] haystack != haystack;
    missing =
      !(contains fromRpath phase)
      || !(contains fromCheck phase)
      || !(contains fromFind phase)
      || !(contains fromCopy phase);
  in
  if missing then
    throw "nix-boehmgc-install: Nix install phase is missing the glibc library check"
  else
    {
      installPhase = builtins.replaceStrings [ fromRpath fromCheck fromFind fromCopy ] [
        toRpath
        toCheck
        toFind
        toCopy
      ] phase;
    }
)
