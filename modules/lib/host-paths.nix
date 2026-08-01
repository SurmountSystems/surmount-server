# Shared host-path validators for deploy secrets, TLS PEMs, and Arti state.
# Paths are operator-placed on the host only (never in git). Strict charset
# avoids shell / Environment= injection via metacharacters.

{ lib }:
let
  inherit (lib) all match;

  # Absolute path: leading /, then only [A-Za-z0-9._/-], no ".." segments,
  # no whitespace, quotes, $, backticks, or newlines.
  # Empty string is allowed when the option is optional (caller decides).
  strictHostPath =
    p:
    p != "" && match "/[A-Za-z0-9._/-]+" p != null && !(lib.hasInfix ".." p) && !(lib.hasInfix "//" p);

  optionalStrictHostPath = p: p == "" || strictHostPath p;

  allStrictHostPaths = paths: all strictHostPath paths;
in
{
  inherit strictHostPath optionalStrictHostPath allStrictHostPaths;
}
