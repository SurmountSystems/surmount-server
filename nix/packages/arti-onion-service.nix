# Surmount-owned Arti with onion-service-service (hidden service publish).
#
# Why not stock pkgs.arti from the host nixpkgs channel?
#   That channel packages Arti 1.4.2 (client-default features only). Upstream
#   stable is far ahead (2.5.1 as of 2026-08-03; re-check 2026-08-27, no 2.6
#   on crates.io). Greenfield Surmount pins the current engine here, same
#   spirit as Stalwart: channel lag is not a reason to ship old Tor for the
#   required HS surface.
#
# Why not only overrideAttrs on host pkgs.arti?
#   Feature-only overrides inherit the channel version (1.4.2). This expression
#   owns the upstream tag (arti-v2.5.1), features, and Surmount capability
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
#   FODs). We still build *from GitLab source*, but take `cargoDeps` from the
#   matching nixpkgs-rust `arti` package (same version / Cargo.lock) so the
#   vendor tree is substitutable from cache.nixos.org. When crates.io vendor
#   works again, cargoHash can replace the artiUnstable.cargoDeps handoff.
#
# Toolchain:
#   Arti 2.5.1 MSRV is 1.91. Older host channels had rustc < 1.91 and
#   rustPackages_* tops out at 1.89. The flake passes a rustPlatform whose
#   rustc/cargo meet MSRV (from the nixpkgs-rust input), without rebasing
#   the whole host OS channel.
#
# Features: onion-service-service on the arti binary (HS publish / tor-hsrproxy).
#   Prefer this lean feature over nixpkgs-unstable `full` unless build needs
#   expand. arti-client's matching feature pulls keymgr as needed for HS
#   identity. Do not enable the arti binary experimental top-level `keymgr`
#   unless a future residual proves it is required beyond onion-service-service.
#
# Never put HS private keys in this expression or the repo.
# Live Tor publish still requires operator host keys + network verify (residual).
#
# Bump procedure:
#   1. Set the single `version` binding below to "X.Y.Z" (GitLab tag
#      arti-vX.Y.Z). Assert, drv version, and src tag all use that binding.
#   2. Bump flake input nixpkgs-rust until its pkgs.arti.version matches (for
#      cargoDeps), or restore cargoHash once crates.io vendor works.
#   3. Refresh src hash (nix build until got: hash, or prefetch).
#   4. Confirm MSRV still met by nixpkgs-rust rustc.
#   5. Re-read upstream CHANGELOG / example config for TOML shape drift.
#   6. Keep passthru.surmountOnionServiceCapable = true.
#
# Modelled on nixpkgs-unstable pkgs/by-name/ar/arti/package.nix (2.5.1 shape:
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
  versionCheckHook,
  # Matching-version arti from nixpkgs-rust (provides substitutable cargoDeps).
  artiUnstable,
}:

let
  # Single source of truth for pin, cargoDeps assert, and GitLab tag.
  version = "2.5.1";
in
assert lib.assertMsg (
  artiUnstable != null
) "arti-onion-service: artiUnstable is required (cargoDeps handoff)";
assert lib.assertMsg (artiUnstable.version == version)
  "arti-onion-service: artiUnstable.version must be ${version} (got ${artiUnstable.version or "null"})";

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "arti-onion-service";
  inherit version;

  src = fetchFromGitLab {
    domain = "gitlab.torproject.org";
    group = "tpo";
    owner = "core";
    repo = "arti";
    tag = "arti-v${finalAttrs.version}";
    hash = "sha256-fPobYu2ADTeIwpeXyxQKh5yr1zw+yMQfqTkiZMMd8YY=";
  };

  # Working around a bug in cargo that appears with cargo-auditable, see
  # https://github.com/rust-secure-code/cargo-auditable/issues/124.
  postPatch = ''
    substituteInPlace crates/arti/Cargo.toml \
      --replace-fail '"tokio-util"' '"dep:tokio-util"'
  '';

  buildAndTestSubdir = "crates/arti";

  # Substitutable vendor tree from nixpkgs-rust arti (avoids crates.io 403).
  # Same version lock as src tag above.
  cargoDeps = artiUnstable.cargoDeps;

  nativeBuildInputs = lib.optionals stdenv.hostPlatform.isLinux [ pkg-config ];

  buildInputs = [ sqlite ] ++ lib.optionals stdenv.hostPlatform.isLinux [ openssl ];

  # Lean HS publish surface (not nixpkgs-unstable `full`).
  # buildRustPackage maps buildFeatures -> cargoBuildFeatures on the drv.
  buildFeatures = [ "onion-service-service" ];
  checkFeatures = [ "onion-service-service" ];

  checkFlags = [
    # problematic test that hangs the build (same skip as nixpkgs 2.5.1)
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
      mode = "gitlab-source-with-nixpkgs-rust-cargoDeps";
      upstreamTag = "arti-v${finalAttrs.version}";
      reason = "crates.io fetch-cargo-vendor 403; reuse matching nixpkgs-rust vendor";
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
