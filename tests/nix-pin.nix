# Prints the mail-host Nix pin from nix/packages/nix-upstream-binary.nix.
# The pin is the upstream 2.36.0 binary tarball. It is not nix-cli
# and not nix-everything.
# Strict eval only. Does not build the mail host and does not read flake.nix.
#
#   nice -n 19 nix-instantiate --eval --strict tests/nix-pin.nix
let
  text = builtins.readFile ../nix/packages/nix-upstream-binary.nix;
  lines = builtins.filter builtins.isString (builtins.split "\n" text);

  matches = needle: builtins.filter (line: builtins.match needle line != null) lines;
  one =
    label: xs:
    if xs == [ ] then
      throw ("nix-pin: " + label + " was not found")
    else if builtins.length xs != 1 then
      throw ("nix-pin: " + label + " matched more than once")
    else
      builtins.head xs;

  version = one "version" (matches "[[:space:]]*version = \"2\\.36\\.0\";[[:space:]]*");
  url = one "url" (
    matches "[[:space:]]*url = \"https://releases\\.nixos\\.org/nix/nix-2\\.36\\.0/nix-2\\.36\\.0-x86_64-linux\\.tar\\.xz\";[[:space:]]*"
  );
  hash = one "hash" (
    matches "[[:space:]]*hash = \"sha256-xNGu0Ij2Kqcpgjz/oyaeiA3mzmEC/suJCfKCHu2y8us=\";[[:space:]]*"
  );
  overlays = builtins.readFile ../nix/overlays.nix;
  overlayLines = builtins.filter builtins.isString (builtins.split "\n" overlays);
  sourcePin = builtins.filter (
    line: builtins.match ".*nixComponents_git\\.nix-cli.*" line != null
  ) overlayLines;
  everything = builtins.filter (
    line: builtins.match "[[:space:]]*nix = .*nix-everything.*" line != null
  ) overlayLines;
in
assert sourcePin == [ ];
assert everything == [ ];
{
  inherit version url hash;
  binaryTarball = true;
}
