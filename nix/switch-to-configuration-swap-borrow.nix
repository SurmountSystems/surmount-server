# Clippy compiles pkgs.switch-to-configuration-ng src/main.rs under
# #![deny(clippy::all)]. The two swap eprintln lines pass &device.
# That borrow is redundant. Do not allow the lint. Do not copy the crate.
{ prev }:
prev.switch-to-configuration-ng.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    f=src/main.rs
    if [ ! -f "$f" ]; then
      echo "switch-to-configuration src/main.rs is missing" >&2
      exit 1
    fi
    would=$(grep -n -F 'eprintln!("would stop swap device: {}", &device);' "$f" || true)
    stopping=$(grep -n -F 'eprintln!("stopping swap device: {}", &device);' "$f" || true)
    if [ -z "$would" ] || [ -z "$stopping" ]; then
      echo "swap eprintln borrows are not in the clippy source" >&2
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
    if ! grep -F -q 'eprintln!("would stop swap device: {}", device);' "$f"; then
      echo "would-stop swap eprintln was not rewritten" >&2
      exit 1
    fi
    if ! grep -F -q 'eprintln!("stopping swap device: {}", device);' "$f"; then
      echo "stopping swap eprintln was not rewritten" >&2
      exit 1
    fi
  '';
})
