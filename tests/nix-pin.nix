# Prints the mail-host Nix pin from nix/overlays.nix.
# The pin is the nixos-26.05 nix-cli source build. It is not the
# upstream binary tarball and not nix-everything.
# Strict eval only. Does not build the mail host and does not read flake.nix.
#
#   nice -n 19 nix-instantiate --eval --strict tests/nix-pin.nix
let
  text = builtins.readFile ../nix/overlays.nix;
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

  pin = one "nix-cli source pin" (
    matches "[[:space:]]*nix = prev\\.nixVersions\\.nixComponents_git\\.nix-cli;[[:space:]]*"
  );
  tarball = matches ".*releases\\.nixos\\.org/nix/nix-.*";
  everything = matches "[[:space:]]*nix = .*nix-everything.*";
in
assert tarball == [ ];
assert everything == [ ];
{
  pin = pin;
  binaryTarball = false;
}
