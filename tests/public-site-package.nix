# Eval-only contracts for the packaged SurmountSystems/site static tree.
# Does not fetch git; uses the flake-passed src (locked GitHub rev).
#
# Call from flake:
#   import ./tests/public-site-package.nix { inherit pkgs lib src; }
#
# Named contract: store package contains current public site copy
# (title Surmount Systems, Bitcoin-focused Deep Tech, grok-oss harness),
# not UNDER CONSTRUCTION.

{
  pkgs,
  lib,
  src,
}:
let
  pkg = pkgs.callPackage ../nix/packages/surmount-public-site.nix { inherit src; };

  index = builtins.readFile (pkg + "/index.html");
  philosophy = builtins.readFile (pkg + "/philosophy.html");

  t0 =
    assert pkg.pname == "surmount-public-site";
    "t0-pname-ok";

  t1 =
    assert builtins.pathExists (pkg + "/index.html");
    assert builtins.pathExists (pkg + "/philosophy.html");
    assert builtins.pathExists (pkg + "/projects.html");
    assert builtins.pathExists (pkg + "/faith.html");
    assert builtins.pathExists (pkg + "/vocabulary.html");
    assert builtins.pathExists (pkg + "/contact.html");
    assert builtins.pathExists (pkg + "/styles.css");
    assert builtins.pathExists (pkg + "/nav.js");
    assert builtins.pathExists (pkg + "/support.html");
    assert builtins.pathExists (pkg + "/contributors.html");
    "t1-required-files-ok";

  t2 =
    assert lib.hasInfix "<title>Surmount Systems</title>" index;
    assert lib.hasInfix "Bitcoin-focused Deep Tech" index;
    assert lib.hasInfix "grok-oss" index;
    assert lib.hasInfix "Unlicense" index;
    assert !(lib.hasInfix "UNDER CONSTRUCTION" index);
    "t2-index-copy-ok";

  t3 =
    assert lib.hasInfix "Philosophy - Surmount Systems" philosophy;
    "t3-philosophy-ok";

  results = [
    t0
    t1
    t2
    t3
  ];
in
{
  inherit results;
  ok = results;
}
