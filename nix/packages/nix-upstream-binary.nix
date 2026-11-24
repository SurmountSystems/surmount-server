# Upstream Nix 2.36.0 x86_64-linux binary tarball.
# URL and hash are the pin that was already recorded for this tree.
# The dynamic loader is the mail-host glibc (2.44 or newer), not the
# glibc path baked into the archive and not nixos-26.05's 2.42.
# No Python, no Meson, and no C program.
{
  stdenvNoCC,
  fetchurl,
  patchelf,
  glibc,
}:

assert stdenvNoCC.hostPlatform.isx86_64;

stdenvNoCC.mkDerivation {
  pname = "nix";
  version = "2.36.0";

  src = fetchurl {
    url = "https://releases.nixos.org/nix/nix-2.36.0/nix-2.36.0-x86_64-linux.tar.xz";
    hash = "sha256-xNGu0Ij2Kqcpgjz/oyaeiA3mzmEC/suJCfKCHu2y8us=";
  };

  nativeBuildInputs = [ patchelf ];

  dontConfigure = true;
  dontBuild = true;
  dontPatchELF = true;
  dontStrip = true;

  installPhase = ''
    set -eu
    set -o pipefail

    nixbin=$(find . -type f -path '*/bin/nix' -print)
    count=$(printf '%s\n' "$nixbin" | grep -c . || true)
    if [ "$count" -ne 1 ]; then
      echo "expected one bin/nix regular file, found $count" >&2
      exit 1
    fi
    root=$(dirname "$(dirname "$nixbin")")
    test -d "$root/bin"
    test -d "$root/lib"

    mkdir -p "$out/bin" "$out/lib"
    cp -a "$root/bin/." "$out/bin/"
    if [ -d "$root/libexec" ]; then
      mkdir -p "$out/libexec"
      cp -a "$root/libexec/." "$out/libexec/"
    fi
    if [ -d "$root/lib/tmpfiles.d" ]; then
      mkdir -p "$out/lib/tmpfiles.d"
      cp -a "$root/lib/tmpfiles.d/." "$out/lib/tmpfiles.d/"
    fi
    if [ -d "$root/share" ]; then
      mkdir -p "$out/share"
      cp -a "$root/share/." "$out/share/"
    fi

    interp="${glibc}/lib/ld-linux-x86-64.so.2"
    test -e "$interp"
    rpath="${glibc}/lib:$out/lib"

    list="$TMPDIR/nix-elf-list"
    : > "$list"
    find "$out/bin" -type f >> "$list"
    if [ -d "$out/libexec" ]; then
      find "$out/libexec" -type f >> "$list"
    fi

    i=1
    while true; do
      file=$(sed -n "''${i}p" "$list")
      if [ -z "$file" ]; then
        break
      fi
      i=$((i + 1))
      if [ -L "$file" ] || [ ! -f "$file" ]; then
        continue
      fi
      if ! patchelf --print-needed "$file" >/dev/null 2>&1; then
        continue
      fi
      chmod u+w "$file"
      if patchelf --print-interpreter "$file" >/dev/null 2>&1; then
        patchelf --set-interpreter "$interp" "$file"
      fi
      patchelf --set-rpath "$rpath" "$file"
      needs=$(patchelf --print-needed "$file")
      IFS='
'
      for need in $needs; do
        [ -z "$need" ] && continue
        case "$need" in
          /nix/store/*) base=$(basename "$need") ;;
          *) base=$need ;;
        esac
        case "$base" in
          ld-linux*) continue ;;
          *python*) continue ;;
        esac
        if [ -e "${glibc}/lib/$base" ]; then
          if [ "$need" != "$base" ]; then
            patchelf --replace-needed "$need" "$base" "$file"
          fi
          continue
        fi
        if [ ! -e "$out/lib/$base" ]; then
          src=$(find . -type f -name "$base" ! -path '*/share/*' ! -path '*/python/*' -print | head -n 1)
          if [ -z "$src" ]; then
            echo "needed library $need is not in the archive or in glibc" >&2
            exit 1
          fi
          cp -a "$src" "$out/lib/$base"
          chmod u+w "$out/lib/$base"
          printf '%s\n' "$out/lib/$base" >> "$list"
        fi
        if [ "$need" != "$base" ]; then
          patchelf --replace-needed "$need" "$base" "$file"
        fi
      done
      unset IFS
    done

    got=$(patchelf --print-interpreter "$out/bin/nix")
    if [ "$got" != "$interp" ]; then
      echo "nix interpreter is $got" >&2
      exit 1
    fi
    test -e "$out/lib/libnixstore.so.2.36.0"
    test -e "$out/lib/libnixutil.so.2.36.0"
    test -e "$out/bin/nix-daemon"
    test -x "$out/bin/nix-store"
    if patchelf --print-needed "$out/lib/libnixstore.so.2.36.0" | grep -F '/nix/store/' >/dev/null; then
      echo "libnixstore still has an absolute needed entry" >&2
      exit 1
    fi
  '';

  meta = {
    description = "Upstream Nix 2.36.0 binary, linked to the mail-host glibc";
    platforms = [ "x86_64-linux" ];
    mainProgram = "nix";
  };
}
