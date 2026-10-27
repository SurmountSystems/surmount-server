# Mail-host C and C++ builds, including their libraries, request
# x86-64-v3. When this module is enabled it sets
# nixpkgs.hostPlatform.gcc.arch. The cc-wrapper reads that into
# cc-cflags-before, so dependency builds (nixpkgs RocksDB, libsodium,
# bitcoind and elementsd libraries, and the rest of this host's
# stdenv) get -march=x86-64-v3. The next switch rebuilds that closure.
# CI, the generic eval host, and flake packages.* (mkPkgsBare) leave
# the module off, so they stay untuned.
#
# This nixpkgs does not map gcc.arch to rustc -C target-cpu. Named
# top-level derivations still get CARGO_BUILD_RUSTFLAGS. Stalwart is
# a release binary (dontBuild). Do not retune that tarball or the
# RocksDB inside it. Do not use -march=native. Do not request
# x86-64-v4 or avx512.

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
      compile mail-host C and C++ for x86-64-v3, including libraries
      built for this host. Sets nixpkgs.hostPlatform.gcc.arch when
      enabled. Off for the generic CI host and for flake packages.*.
    '';

    gccArch = mkOption {
      type = types.str;
      default = "";
      description = ''
        gcc.arch token for mail-host source builds. Empty when enable
        is false. When enable is true this is also
        nixpkgs.hostPlatform.gcc.arch, which the cc-wrapper applies
        to every C and C++ compile in this host's package set.
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

    # cc-wrapper machineFlags reads targetPlatform.gcc.arch and writes
    # -march into nix-support/cc-cflags-before. That covers dependency
    # builds. It does not retune a dontBuild release binary. The named
    # overlay below still sets Rust target-cpu; gcc.arch does not.
    nixpkgs.hostPlatform = {
      system = config.nixpkgs.system;
      gcc.arch = march;
    };

    # The v3 stdenv requires gccarch-x86-64-v3. extra-system-features is
    # a freeform list and concatenates, so this does not drop
    # surmount-remote or the daemon's system-features defaults.
    nix.settings.extra-system-features = [ "gccarch-${march}" ];

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
      {
        assertion =
          (config.nixpkgs.hostPlatform.gcc.arch or null) == march
          && (config.nixpkgs.hostPlatform.system or "") == "x86_64-linux"
          && !(lib.hasInfix "native" (config.nixpkgs.hostPlatform.gcc.arch or ""))
          && (config.nixpkgs.hostPlatform.gcc.arch or "") != "x86-64-v4"
          && !(lib.hasInfix "avx512" (config.nixpkgs.hostPlatform.gcc.arch or ""));
        message = "surmount.mailHostCpu must set nixpkgs.hostPlatform.gcc.arch to x86-64-v3 on x86_64-linux, not native, v4, or avx512.";
      }
    ];
  };
}
