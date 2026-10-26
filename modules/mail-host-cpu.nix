# Mail-host source builds request x86-64-v3. CI and generic laptop
# package builds do not. nixpkgs.hostPlatform.gcc.arch stays unset:
# the cc-wrapper bakes that into every NixOS derivation and would
# rebuild the closure. Per-derivation NIX_CFLAGS_COMPILE is the same
# -march the wrapper would add from gcc.arch, and this nixpkgs does
# not map gcc.arch to rustc -C target-cpu.
#
# Stalwart is a release binary (dontBuild). Do not retune it.
# Do not use -march=native. Do not request x86-64-v4 or avx512.

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount.mailHostCpu;
  inherit (lib) mkIf mkOption types;
  march = "x86-64-v3";
  nixCflagsCompile = "-march=${march}";
  cargoBuildRustflags = "-C target-cpu=${march}";
  # Source builds that can land in the mail-host closure.
  # bitcoind and elementsd are tuned only if something references them.
  # This module does not enable them and does not start them.
  tunedPackages = [
    "surmount-management-ui"
    "artiOnionService"
    "acme-dns-hook-namecheap"
    "surmount-private-data"
    "surmount-host-logs"
    "surmount-shc"
    "surmount-niced-builder"
    "surmount-scram"
    "surmount-leftover-homes"
    "surmount-host-probe"
    "surmount-static-sites"
    "surmount-diskstation"
    "surmount-deploy-host"
    "surmount-dns-zone"
    "surmount-domain-audit"
    "surmount-host-cutover"
    "surmount-acme-namecheap"
    "surmount-stalwart-ops"
    "surmount-mail-import"
    "surmount-secrets-install"
    "surmount-secrets-prompt"
    "surmount-rekey"
    "splora"
    "splora-liquid"
    "bitcoind"
    "elementsd"
  ];

  addFlag =
    flag: old: name:
    let
      top = if old ? ${name} && old.${name} != null then toString old.${name} else "";
      fromEnv =
        if (old.env or { }) ? ${name} && old.env.${name} != null then toString old.env.${name} else "";
      combined = lib.concatStringsSep " " (
        lib.filter (s: s != "") [
          top
          fromEnv
        ]
      );
    in
    if flag == "" then
      combined
    else if lib.hasInfix flag combined then
      combined
    else
      lib.concatStringsSep " " (
        lib.filter (s: s != "") [
          flag
          combined
        ]
      );

  tuneDrv =
    pkg:
    if !(lib.isDerivation pkg) then
      pkg
    else
      pkg.overrideAttrs (old: {
        env = (old.env or { }) // {
          NIX_CFLAGS_COMPILE = addFlag nixCflagsCompile old "NIX_CFLAGS_COMPILE";
          NIX_CXXFLAGS_COMPILE = addFlag nixCflagsCompile old "NIX_CXXFLAGS_COMPILE";
          CARGO_BUILD_RUSTFLAGS = addFlag cargoBuildRustflags old "CARGO_BUILD_RUSTFLAGS";
        };
      });

  artifactsOf =
    pkg:
    if pkg ? cargoArtifacts && pkg.cargoArtifacts != null then
      pkg.cargoArtifacts
    else if pkg ? env && pkg.env ? cargoArtifacts then
      pkg.env.cargoArtifacts
    else
      null;

  tune =
    pkg:
    let
      flagged = tuneDrv pkg;
      artifacts = artifactsOf pkg;
    in
    if lib.isDerivation artifacts then
      flagged.overrideAttrs (_old: {
        cargoArtifacts = tuneDrv artifacts;
      })
    else
      flagged;

  # mkAfter so this runs after the flake surmountOverlay, which
  # replaces these attrs with the generic package set.
  mailHostCpuOverlay =
    _final: prev:
    lib.foldl' lib.recursiveUpdate { } (
      map (
        name:
        lib.optionalAttrs (prev ? ${name}) {
          ${name} = tune prev.${name};
        }
      ) tunedPackages
    );
in
{
  options.surmount.mailHostCpu = {
    enable = lib.mkEnableOption ''
      compile mail-host Rust and C/C++ source builds for x86-64-v3.
      Off for the generic CI host. Does not set nixpkgs.hostPlatform.
    '';

    gccArch = mkOption {
      type = types.str;
      default = "";
      description = ''
        gcc.arch token for mail-host source builds. Empty when enable
        is false. Not nixpkgs.hostPlatform.gcc.arch.
      '';
    };

    rustTargetCpu = mkOption {
      type = types.str;
      default = "";
      description = ''
        rustc -C target-cpu value for mail-host source builds.
        Empty when enable is false.
      '';
    };

    nixCflagsCompile = mkOption {
      type = types.str;
      default = "";
      description = ''
        NIX_CFLAGS_COMPILE / NIX_CXXFLAGS_COMPILE prefix for mail-host
        source builds. The nixpkgs cc-wrapper applies NIX_CFLAGS_COMPILE
        to C and C++. Empty when enable is false.
      '';
    };

    cargoBuildRustflags = mkOption {
      type = types.str;
      default = "";
      description = ''
        CARGO_BUILD_RUSTFLAGS prefix for mail-host source builds.
        Empty when enable is false.
      '';
    };

    tunedPackages = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = ''
        Package attribute names that receive the mail-host flags when
        enable is true and the attribute exists. Stalwart's release
        binary is not in this list.
      '';
    };
  };

  config = mkIf cfg.enable {
    surmount.mailHostCpu.gccArch = march;
    surmount.mailHostCpu.rustTargetCpu = march;
    surmount.mailHostCpu.nixCflagsCompile = nixCflagsCompile;
    surmount.mailHostCpu.cargoBuildRustflags = cargoBuildRustflags;
    surmount.mailHostCpu.tunedPackages = tunedPackages;

    nixpkgs.overlays = lib.mkAfter [ mailHostCpuOverlay ];

    assertions = [
      {
        assertion =
          cfg.gccArch == march
          && cfg.rustTargetCpu == march
          && cfg.nixCflagsCompile == nixCflagsCompile
          && cfg.cargoBuildRustflags == cargoBuildRustflags
          && !(lib.hasInfix "native" cfg.nixCflagsCompile)
          && !(lib.hasInfix "native" cfg.cargoBuildRustflags)
          && !(lib.hasInfix "x86-64-v4" cfg.gccArch)
          && !(lib.hasInfix "avx512" cfg.cargoBuildRustflags);
        message = "surmount.mailHostCpu must request x86-64-v3, not native, v4, or avx512.";
      }
    ];
  };
}
