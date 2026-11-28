# Clippy compiles pkgs.switch-to-configuration-ng src/main.rs under
# #![deny(clippy::all)]. The two swap eprintln lines must pass device
# with no redundant borrow. A tarball that still writes &device on
# those two calls is rewritten to device. A tarball that already
# passes device is left as it is. Do not allow the lint. Do not copy
# the crate. Do not match new_swaps.contains_key(&device).
{ prev }:
prev.switch-to-configuration-ng.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    f=src/main.rs
    if [ ! -f "$f" ]; then
      echo "switch-to-configuration src/main.rs is missing" >&2
      exit 1
    fi
    sed -i \
      -e 's/eprintln!("would stop swap device: {}", \&device);/eprintln!("would stop swap device: {}", device);/' \
      -e 's/eprintln!("stopping swap device: {}", \&device);/eprintln!("stopping swap device: {}", device);/' \
      "$f"
    if grep -F -q 'eprintln!("would stop swap device: {}", &device);' "$f"; then
      echo "would-stop swap eprintln still borrows device" >&2
      exit 1
    fi
    if grep -F -q 'eprintln!("stopping swap device: {}", &device);' "$f"; then
      echo "stopping swap eprintln still borrows device" >&2
      exit 1
    fi
    would=$(grep -n -F 'eprintln!("would stop swap device: {}", device);' "$f" || true)
    stopping=$(grep -n -F 'eprintln!("stopping swap device: {}", device);' "$f" || true)
    if [ -z "$would" ]; then
      echo "would-stop swap eprintln is missing from the clippy source" >&2
      exit 1
    fi
    if [ -z "$stopping" ]; then
      echo "stopping swap eprintln is missing from the clippy source" >&2
      exit 1
    fi
    case "$would" in
      *"//"*|*"/*"*)
        echo "would-stop match is not the eprintln call" >&2
        exit 1
        ;;
    esac
    case "$stopping" in
      *"//"*|*"/*"*)
        echo "stopping match is not the eprintln call" >&2
        exit 1
        ;;
    esac
    echo "swap eprintln lines pass device with no redundant borrow"
  '';
})
