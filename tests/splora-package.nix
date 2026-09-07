# Eval-only contracts for flake-input Splora packages.
# Does not build splora or splora-deps (no rocksdb, no cargo, no guest OOM).
#
# Call from flake: import ./tests/splora-package.nix {
#   inherit lib sploraFlake system;
# }

{
  lib,
  sploraFlake,
  system,
}:
let
  pkgsFor = sploraFlake.packages.${system} or { };
  splora =
    pkgsFor.splora or (throw "splora flake input has no packages.${system}.splora (fail-closed)");
  liquid =
    pkgsFor.splora-liquid
      or (throw "splora flake input has no packages.${system}.splora-liquid (fail-closed)");

  origCargoConfigPath = sploraFlake.outPath + "/.cargo/config.toml";
  origCargoConfig =
    if builtins.pathExists origCargoConfigPath then builtins.readFile origCargoConfigPath else "";

  t0 =
    assert lib.hasInfix "menhera-cooldown" origCargoConfig;
    assert lib.hasInfix "index.crates.menhera.org" origCargoConfig;
    "t0-input-cargo-config-has-menhera-for-laptop-cargo-ok";

  t1 =
    assert !(splora.passthru.nixCompileStripsMenheraReplaceWith or false);
    assert !(liquid.passthru.nixCompileStripsMenheraReplaceWith or false);
    "t1-no-this-tree-menhera-src-wrap-ok";

  t2 =
    assert splora.pname == "splora";
    assert liquid.pname == "splora-liquid";
    "t2-overlay-attr-names-ok";

  results = [
    t0
    t1
    t2
  ];
in
{
  inherit results;
  ok = results;
}
