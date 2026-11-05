# ssh-ng remote builder with a hard memory cap on the real rustc cgroup.
# Trusted ssh-ng stdio forwards builds to system nix-daemon. Cap and nice
# nix-daemon.service, not only the nixbuilder user slice.
# Niceness is not a memory cap. SHC ticket 261 (closed 2026-08-18): guest
# RAM exhaustion triggered a Proxmox OOM-shutdown. Cap this path only.
# Mail, management-ui, sshd, Arti, and networking stay unstarved.
# Optional Lake is modules/lake.nix (default off). This module still
# refuses a surmount-lake unit unless surmount.lake.enable is true.

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  rb = cfg.remoteBuilder;
  inherit (lib) mkIf;
  sliceName = "surmount-builder";
  userSliceName = "user-${toString rb.uid}";
  helperEtc = "surmount/niced-builder";
  helperPath = "/etc/${helperEtc}";
  nixDaemon = "${pkgs.nix}/bin/nix-daemon";
  cpuQuotaAuto = rb.cpuQuota == "auto" || rb.cpuQuota == "";
  sliceCpu =
    if !cpuQuotaAuto then
      { CPUQuota = rb.cpuQuota; }
    else if rb.onlineCpus != null then
      { CPUQuota = "${toString (95 * rb.onlineCpus)}%"; }
    else
      { };
  commonEnv = ''
    export SURMOUNT_BUILDER_MEMORY_MAX=${lib.escapeShellArg rb.memoryMax}
    export SURMOUNT_BUILDER_CPU_QUOTA=${lib.escapeShellArg rb.cpuQuota}
    export SURMOUNT_BUILDER_DISK_GUARD_PERCENT=${toString rb.diskGuardPercent}
    export SURMOUNT_BUILDER_DISK_GUARD_PATH=${lib.escapeShellArg rb.diskGuardPath}
    export SURMOUNT_BUILDER_SLICE=${sliceName}.slice
  '';
  # nixbuilder login profile only. Not nix-daemon Environment and not the
  # sandbox PATH (derivations keep their own stdenv). pkgs.gcc is wrapCC
  # (bin/cc, bin/c++). clang-unwrapped, not pkgs.clang, because the wrapped
  # clang also ships cc. Lean is nix/packages/lean4.nix (v4.35.0),
  # not pkgs.lean4 from this nixpkgs (4.29.1) and not an elan pin.
  gccBin = pkgs.gcc;
  clangBin = pkgs.llvmPackages.clang-unwrapped;
  binutilsBin = pkgs.binutils;
  lldBin = pkgs.lld;
  makeBin = pkgs.gnumake;
  leanBin = pkgs.callPackage ../nix/packages/lean4.nix { };
  builderDevtools = pkgs.runCommand "surmount-builder-devtools" { } ''
    set -eu
    mkdir -p "$out/bin"
    test -e ${gccBin}/bin/cc
    test -e ${gccBin}/bin/c++
    test -e ${gccBin}/bin/gcc
    test -e ${gccBin}/bin/g++
    test -e ${clangBin}/bin/clang
    test -e ${binutilsBin}/bin/ld
    test -e ${makeBin}/bin/make
    test -e ${leanBin}/bin/lean
    test -e ${leanBin}/bin/lake
    test -e ${leanBin}/bin/leanc
    test -e ${leanBin}/bin/leanmake
    ln -s ${gccBin}/bin/cc "$out/bin/cc"
    ln -s ${gccBin}/bin/c++ "$out/bin/c++"
    ln -s ${gccBin}/bin/gcc "$out/bin/gcc"
    ln -s ${gccBin}/bin/g++ "$out/bin/g++"
    ln -s ${clangBin}/bin/clang "$out/bin/clang"
    if [ -e ${clangBin}/bin/clang++ ]; then
      ln -s ${clangBin}/bin/clang++ "$out/bin/clang++"
    else
      # argv[0] clang++ is C++ mode when the unwrapped tree has no symlink.
      ln -s ${clangBin}/bin/clang "$out/bin/clang++"
    fi
    ln -s ${binutilsBin}/bin/ld "$out/bin/ld"
    if [ -e ${lldBin}/bin/lld ]; then
      ln -s ${lldBin}/bin/lld "$out/bin/lld"
    fi
    if [ -e ${lldBin}/bin/ld.lld ]; then
      ln -s ${lldBin}/bin/ld.lld "$out/bin/ld.lld"
    elif [ -e ${lldBin}/bin/lld ]; then
      ln -s ${lldBin}/bin/lld "$out/bin/ld.lld"
    else
      echo "pkgs.lld has neither bin/lld nor bin/ld.lld" >&2
      exit 1
    fi
    ln -s ${makeBin}/bin/make "$out/bin/make"
    # Wrappers, not symlinks. leanmake uses dirname of its own path
    # to find share/lean/lean.mk. A symlink would point that at this
    # devtools tree. exec of the pinned file keeps that path.
    for tool in lean lake leanc leanmake; do
      printf '#!%s\nexec %s "$@"\n' '${pkgs.runtimeShell}' "${leanBin}/bin/$tool" > "$out/bin/$tool"
      chmod +x "$out/bin/$tool"
    done
  '';
in
{
  config = mkIf (cfg.enable && rb.enable) {
    assertions = [
      {
        assertion = rb.user != "root";
        message = "surmount.remoteBuilder.user must not be root (do not cap or nice the whole machine).";
      }
      {
        assertion = rb.memoryMax != null && rb.memoryMax != "";
        message = "surmount.remoteBuilder.memoryMax must be a non-empty systemd MemoryMax string when enable is true (empty/null refuse; scaffold default 4G is a builder budget, not 95 percent of the whole guest; never publish the guest size in git).";
      }
      {
        assertion = builtins.substring 0 1 rb.diskGuardPath == "/";
        message = "surmount.remoteBuilder.diskGuardPath must be an absolute path.";
      }
      {
        assertion = cfg.lake.enable || !(config.systemd.services ? surmount-lake);
        message = "surmount-lake is only allowed when surmount.lake.enable is true (default off, niced, MemoryMax). Do not overlay a Lake unit that starts without that option.";
      }
    ];

    users.groups.${rb.group} = {
      gid = lib.mkDefault rb.uid;
    };

    users.users.${rb.user} = {
      isNormalUser = true;
      description = "Nix distributed builder (key-only)";
      group = rb.group;
      uid = rb.uid;
      home = "/home/${rb.user}";
      createHome = true;
      hashedPassword = "!";
      linger = true;
      # Activation links this to /etc/profiles/per-user/<user>/bin, which
      # is already on the nixbuilder ssh PATH. A switch rebuilds that
      # profile from this list. Not the nix-daemon unit PATH and not
      # the sandbox.
      packages = [ builderDevtools ];
    };

    # Laptop ssh-ng SSHes as this user. Do not add nixbuilder to wheel.
    nix.settings.extra-trusted-users = [ rb.user ];
    # Job slots stay under MemoryMax. Half the measured mail-host CPUs,
    # not one slot per CPU. Laptop machines max-jobs must match.
    nix.settings.max-jobs = rb.maxJobs;
    nix.settings.cores = lib.mkIf (rb.buildCores != null) rb.buildCores;
    # Client machines-file already lists surmount-remote. The builder
    # nix-daemon must list it too or rustc is not eligible here. extra-
    # keeps NixOS auto-detected features (big-parallel).
    nix.settings.extra-system-features = [ "surmount-remote" ];

    # Trusted ssh-ng forwards rustc here. Cap and nice this cgroup.
    # Do not Nice= mail, management-ui, sshd, Arti, or networking.
    # SHC 261/262: prefer killing the builder over mail when the guest
    # is tight. MemoryMax alone does not protect mail if the builder
    # cap is most of guest RAM.
    systemd.services.nix-daemon.serviceConfig = {
      MemoryAccounting = true;
      MemoryMax = rb.memoryMax;
      Nice = 19;
      IOSchedulingClass = lib.mkForce "idle";
      OOMScoreAdjust = 500;
      SyslogIdentifier = "nix-daemon";
      LogRateLimitIntervalSec = "30s";
      LogRateLimitBurst = 50000;
    }
    // sliceCpu;

    systemd.services.stalwart-mail.serviceConfig.OOMScoreAdjust = -300;
    systemd.services.surmount-management-ui.serviceConfig.OOMScoreAdjust = -300;
    systemd.services.sshd.serviceConfig.OOMScoreAdjust = -300;
    # Do not instantiate the daemon unit when startDaemon is false.
    # A bare serviceConfig assignment would create it with enable defaulting
    # to true. OOM protection still applies when the daemon is requested.
    systemd.services.surmount-arti-hidden-service = mkIf cfg.artiHiddenService.startDaemon {
      serviceConfig.OOMScoreAdjust = -300;
    };

    systemd.slices.${sliceName} = {
      description = "Surmount ssh-ng builder (hard memory cap)";
      sliceConfig = {
        MemoryAccounting = true;
        CPUAccounting = true;
        IOAccounting = true;
        MemoryMax = rb.memoryMax;
      }
      // sliceCpu;
    };

    # pam_systemd puts ssh-ng sessions in user-<uid>.slice. Cap that user
    # so a local stdio rustc cannot eat the last of guest RAM. Do not put
    # a one-CPU CPUQuota here: that starves the protocol and leaves cores idle.
    systemd.slices.${userSliceName} = {
      description = "nixbuilder user slice (hard memory cap)";
      sliceConfig = {
        MemoryAccounting = true;
        CPUAccounting = true;
        IOAccounting = true;
        MemoryMax = rb.memoryMax;
      }
      // sliceCpu;
    };

    systemd.user.slices.${sliceName} = {
      sliceConfig = {
        MemoryAccounting = true;
        CPUAccounting = true;
        IOAccounting = true;
        MemoryMax = rb.memoryMax;
      }
      // sliceCpu;
    };

    environment.etc.${helperEtc} = {
      mode = "0755";
      source = "${pkgs.surmount-niced-builder}/bin/surmount-niced-builder";
    };

    environment.etc."surmount/niced-nix-daemon-stdio" = {
      mode = "0755";
      text = ''
        #!/bin/sh
        # ssh-ng remote-program. Nix passes --stdio.
        # MemoryMax=${rb.memoryMax} CPUQuota=${rb.cpuQuota} via niced-builder.
        set -eu
        ${commonEnv}
        exec ${helperPath} ${nixDaemon} "$@"
      '';
    };
  };
}
