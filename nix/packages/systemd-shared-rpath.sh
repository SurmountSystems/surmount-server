# installCheck runs the installed udevadm. libsystemd-shared-*.so
# lives in $out/lib/systemd. The loader exits 127 when that directory
# is missing from RUNPATH. resolve_elf and add_rpath are defined by
# the systemd postFixup that sources this file.
add_systemd_shared_runpath() {
  shared_dir=$1
  if [ ! -d "$shared_dir" ]; then
    return 0
  fi
  found=0
  for so in "$shared_dir"/libsystemd-shared-*.so; do
    if [ -f "$so" ]; then
      found=1
      break
    fi
  done
  if [ "$found" -ne 1 ]; then
    echo "libsystemd-shared is not installed in $shared_dir" >&2
    exit 1
  fi
  find "$out" -type f -print | while IFS= read -r candidate; do
    resolved=$(resolve_elf "$candidate") || continue
    if patchelf --print-needed "$resolved" | grep -F -q libsystemd-shared-; then
      add_rpath "$resolved" "$shared_dir"
    fi
  done
}
