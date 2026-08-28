# Eval-only contracts for Surmount arti-onion-service package expression.
# Does not build arti (no network Tor, no cargo compile). Proves version pin,
# feature list, and capability passthru are wired on the derivation attrset.
#
# Full module startDaemon path: tests/module-eval.nix (fake packages).
# Call from flake: import ./tests/arti-onion-package.nix {
#   inherit pkgs lib rustPlatform artiUnstable;
# }

{
  pkgs,
  lib,
  rustPlatform ? pkgs.rustPlatform,
  artiUnstable,
}:
let
  pkg = pkgs.callPackage ../nix/packages/arti-onion-service.nix {
    inherit rustPlatform artiUnstable;
  };

  buildFeatures = pkg.cargoBuildFeatures or pkg.buildFeatures or [ ];
  checkFeatures = pkg.cargoCheckFeatures or pkg.checkFeatures or [ ];
  passthruFeatures = pkg.passthru.onionServiceFeatures or [ ];

  capable =
    (pkg.passthru.surmountOnionServiceCapable or false) || (pkg.surmountOnionServiceCapable or false);

  metaText = ''
    ${pkg.meta.description or ""}
    ${pkg.meta.longDescription or ""}
  '';

  noKeyMaterial =
    s:
    !(
      lib.hasInfix "BEGIN PRIVATE" s
      || lib.hasInfix "PRIVATE KEY" s
      || lib.hasInfix "hs_ed25519_secret_key" s
      || lib.hasInfix "BEGIN OPENSSH" s
    );

  # Named contract: Surmount owns current upstream Arti (not nixpkgs 1.4.2 lag).
  t0 =
    assert pkg.version == "2.5.1";
    assert !(lib.hasPrefix "1.4" pkg.version);
    "t0-version-2.5.1-ok";

  t1 =
    assert builtins.elem "onion-service-service" buildFeatures;
    assert builtins.elem "onion-service-service" checkFeatures;
    "t1-onion-service-feature-ok";

  t2 =
    assert capable;
    "t2-capable-passthru-ok";

  t3 =
    assert pkg.pname == "arti-onion-service";
    "t3-distinct-pname-ok";

  t4 =
    assert noKeyMaterial metaText;
    "t4-no-key-material-in-meta-ok";

  t5 =
    # Stock pkgs.arti must not claim Surmount capability.
    assert !((pkgs.arti.passthru.surmountOnionServiceCapable or false));
    assert !((pkgs.arti.surmountOnionServiceCapable or false));
    "t5-stock-arti-not-capable-ok";

  t6 =
    assert builtins.elem "onion-service-service" passthruFeatures;
    "t6-passthru-onion-service-features-ok";

  # Named contract: Surmount owns GitLab-source packaging mode (not a silent
  # return to feature-only overrideAttrs of host nixpkgs 1.4.x).
  packaging = pkg.passthru.surmountArtiPackaging or { };
  t7 =
    assert packaging.mode or null == "gitlab-source-with-nixpkgs-rust-cargoDeps";
    assert packaging.upstreamTag or null == "arti-v${pkg.version}";
    "t7-packaging-provenance-ok";

  results = [
    t0
    t1
    t2
    t3
    t4
    t5
    t6
    t7
  ];
in
{
  inherit results;
  ok = results;
}
