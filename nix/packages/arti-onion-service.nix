# Surmount-owned Arti with onion-service-service (hidden service publish).
#
# Why not stock pkgs.arti from the host nixpkgs channel?
#   That channel packages Arti 1.4.2 (client-default features only). Upstream
#   stable is far ahead (2.8.0, GitLab tag arti-v2.8.0). Greenfield Surmount
#   pins the current engine here, same
#   spirit as Stalwart: channel lag is not a reason to ship old Tor for the
#   required HS surface.
#
# Why not only overrideAttrs on host pkgs.arti?
#   Feature-only overrides inherit the channel version (1.4.2). This expression
#   owns the upstream tag (arti-v2.8.0), features, and Surmount capability
#   passthru.
#
# Why source build (not binary FOD like Stalwart)?
#   Tor Project does not publish multi-arch Arti release binaries comparable
#   to Stalwart's GitHub release tarballs. Official path is cargo from the
#   GitLab monorepo (tag arti-vX.Y.Z).
#
# Vendor / crates.io:
#   fetch-cargo-vendor against crates.io returned HTTP 403 from this
#   environment (same class of failure that forced Stalwart to release-binary
#   FODs). nixpkgs-rust pkgs.arti is still 2.7.0. Do not set cargoDeps to
#   that tree or to any other 2.7.0 vendor. cargoHash for arti-v2.8.0 is
#   sha256-grgIqU1gw1UqeGgR4aR7cAOxBW91F12bHMgOj4pgLcY=. The missing-hash
#   throw is gone. Do not pretend a 2.7.0 vendor tree matches this source.
#
# Toolchain:
#   crates/arti/Cargo.toml in the arti-v2.8.0 tree sets rust-version = "1.92".
#   Older host channels had rustc < 1.91 and rustPackages_* tops out at 1.89.
#   The flake passes a rustPlatform whose rustc/cargo meet MSRV (from the
#   nixpkgs-rust input), without rebasing the whole host OS channel.
#
# Features: onion-service-service on the arti binary (HS publish / tor-hsrproxy).
#   Prefer this lean feature over nixpkgs-unstable `full` unless build needs
#   expand. arti-client's matching feature pulls keymgr as needed for HS
#   identity. Do not enable the arti binary experimental top-level `keymgr`
#   unless a future residual proves it is required beyond onion-service-service.
#   Counter Galois Onion is always on since 2.6.0, so this package does not pass
#   that flag and does not use full.
#
# Never put HS private keys in this expression or the repo.
# Live Tor publish still requires operator host keys + network verify (residual).
#
# Bump procedure:
#   1. Set the single `version` binding below to "X.Y.Z" (GitLab tag
#      arti-vX.Y.Z). The drv version and the src tag both use that binding.
#   2. Prefetch cargoHash for this tag. Do not reuse a 2.7.0 vendor tree.
#   3. Refresh src hash (nix build until got: hash, or prefetch).
#   4. Confirm MSRV still met by nixpkgs-rust rustc.
#   5. Re-read upstream CHANGELOG / example config for TOML shape drift.
#   6. Keep passthru.surmountOnionServiceCapable = true.
#
# Modelled on nixpkgs pkgs/by-name/ar/arti/package.nix (2.7.0 shape). Source
# tag is now arti-v2.8.0.
# buildAndTestSubdir, tokio-util postPatch, ARTI_FS_DISABLE_PERMISSION_CHECKS)
# with Surmount pname, lean HS feature, and capability passthru.
#
# Upstream: https://gitlab.torproject.org/tpo/core/arti
# Research: docs/research/arti-and-secrets-manager.md
# Version audit: docs/research/version-audit.md

{
  lib,
  stdenv,
  rustPlatform,
  fetchFromGitLab,
  pkg-config,
  sqlite,
  openssl,
  zstd,
  versionCheckHook,
  # Still passed by the flake. Not a cargoDeps source for 2.8.0.
  artiUnstable,
}:

let
  # Single source of truth for pin, drv version, and GitLab tag.
  version = "2.8.0";
in
assert lib.assertMsg (
  artiUnstable != null
) "arti-onion-service: artiUnstable is passed by the flake; it is not cargoDeps for ${version}";

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "arti-onion-service";
  inherit version;

  src = fetchFromGitLab {
    domain = "gitlab.torproject.org";
    group = "tpo";
    owner = "core";
    repo = "arti";
    tag = "arti-v${finalAttrs.version}";
    # nix-prefetch-url --unpack of the GitLab archive and of the
    # fetchFromGitLab API URL both printed
    # 0jkhr8d9z9m6sffslvahcqnzdvb61kw8hnlgsdm9xnn6lfb80w12
    # builtins.convertHash to sri printed
    # sha256-InCAlqPG2p5q049aiPgMZu32LWZQbaqd06amnxrKcEo=
    hash = "sha256-InCAlqPG2p5q049aiPgMZu32LWZQbaqd06amnxrKcEo=";
  };

  # Working around a bug in cargo that appears with cargo-auditable, see
  # https://github.com/rust-secure-code/cargo-auditable/issues/124.
  postPatch = ''
    substituteInPlace crates/arti/Cargo.toml \
      --replace-fail '"http"' '"dep:http"' \
      --replace-fail '"tokio-util"' '"dep:tokio-util"' \
      --replace-fail '"opentelemetry-appender-tracing",' ""
  '';

  buildAndTestSubdir = "crates/arti";

  # nixpkgs-rust pkgs.arti is 2.7.0. Do not point cargoDeps at it.
  # cargoHash is the fetchCargoVendor vendor-staging got hash for this tag.
  cargoHash = "sha256-grgIqU1gw1UqeGgR4aR7cAOxBW91F12bHMgOj4pgLcY=";

  nativeBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [ pkg-config ];

  buildInputs = [
    sqlite
    zstd
  ]
  ++ lib.optionals stdenv.hostPlatform.isLinux [ openssl ];

  # Lean HS publish surface (not nixpkgs-unstable `full`).
  # buildRustPackage maps buildFeatures -> cargoBuildFeatures on the drv.
  buildFeatures = [ "onion-service-service" ];
  checkFeatures = [ "onion-service-service" ];

  checkFlags = [
    # problematic test that hangs the build (same skip as nixpkgs 2.7.0; still skipped on 2.8.0)
    "--skip=reload_cfg::test::watch_single_file"
  ];

  # CLI tests validate FS/runtime hardening and break in the Nix sandbox.
  # Does NOT affect downstream users of the installed binary.
  env.ARTI_FS_DISABLE_PERMISSION_CHECKS = 1;

  nativeInstallCheckInputs = [ versionCheckHook ];
  doInstallCheck = true;
  # Binary is still `arti` (upstream mainProgram); pname is Surmount-distinct.
  versionCheckProgram = "${placeholder "out"}/bin/arti";
  versionCheckProgramArg = "--version";

  passthru = {
    # Module fail-closed gate: only this Surmount package claims capability.
    # Stock pkgs.arti must not set this. Operators who point package at stock
    # arti must set packageIsOnionServiceCapable manually (documented footgun).
    surmountOnionServiceCapable = true;
    onionServiceFeatures = [ "onion-service-service" ];
    # Honest packaging provenance for audits.
    surmountArtiPackaging = {
      mode = "gitlab-source-cargoHash-set";
      upstreamTag = "arti-v${finalAttrs.version}";
      reason = "arti 2.8.0 cargoHash is sha256-grgIqU1gw1UqeGgR4aR7cAOxBW91F12bHMgOj4pgLcY=; the missing-hash throw is gone; nixpkgs-rust arti 2.7.0 cargoDeps is not this source";
    };
  };

  meta = {
    description = "Arti with onion-service-service (Surmount HS publish package)";
    longDescription = ''
      Surmount-owned build of upstream Arti ${finalAttrs.version} from Tor
      Project GitLab (tag arti-v${finalAttrs.version}), with cargo feature
      onion-service-service so `arti proxy` can publish Tor onion/hidden
      services (tor-hsrproxy path). Distinct from stock pkgs.arti (client-
      default and often channel-lagged). Pair with Surmount module
      option packageIsOnionServiceCapable (auto when this package is selected
      via passthru.surmountOnionServiceCapable). Never contains HS private keys.
    '';
    mainProgram = "arti";
    homepage = "https://arti.torproject.org/";
    changelog = "https://gitlab.torproject.org/tpo/core/arti/-/blob/arti-v${finalAttrs.version}/CHANGELOG.md";
    license = with lib.licenses; [
      asl20
      mit
    ];
  };
})
