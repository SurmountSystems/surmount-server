# Muon does not emit the ninja target meson-test-prereq.
# The check hook still asks for it, so the library check stays off.
final: prev: {
  pixman = prev.pixman.overrideAttrs (old: {
    doCheck = false;
  });
}
