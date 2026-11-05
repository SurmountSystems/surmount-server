# Optional overlays. The flake injects Surmount packages via surmountOverlay
# in flake.nix (stalwart-mail, stalwart-cli, management-ui, artiOnionService,
# ops bins: private-data / host-logs / shc / niced-builder / leftover-homes /
# host-probe / static-sites / diskstation / deploy-host / dns-zone /
# domain-audit / host-cutover / acme-namecheap / stalwart-ops / mail-import /
# secrets-install, grok-oss, splora / splora-liquid from flake input packages).
# Keep this file for any extra pins that should not live in the flake body.
#
# artiOnionService lives in flake surmountOverlay (distinct from stock
# pkgs.arti). The expression is nix/packages/arti-onion-service.nix
# (GitLab tag arti-v2.8.0). The Arti 2.8.0 cargoHash is set there
# (sha256-grgIqU1gw1UqeGgR4aR7cAOxBW91F12bHMgOj4pgLcY=). This file
# does not vendor Arti. Not a feature-only override of channel 1.4.2.
#
# This file exports two overlays:
#   pins   Nix 2.35.4 cli. Safe on checks (mkPkgs) and on the mail host.
#   tzdata tzdata 2026e doCheck = false. Mail-host nixosSystem only.
#          Not on mkPkgs, so checks.x86_64-linux.ci does not rebuild
#          CPython just because now.ck is skipped.
#
# tzdata 2026e: now.ck is date-dependent. On or after 2026-11-01 the check
# fails because America/Phoenix and America/Vancouver are identical from
# now on (America/Edmonton and America/Winnipeg are the same class). This
# is not a gcc or x86-64-v3 failure. Same attribute change as
# NixOS/nixpkgs pull 582069. gcc.arch and x86-64-v3 stay as they are.
#
# Nix: nixos-26.05 pkgs.nix is nixVersions.stable (the 2.34.9 recipe).
# nixVersions.nix_2_35 is the 2.35.3 recipe: one fetchFromGitHub source
# and meson components, no cargoHash. 2.35.4 is that recipe plus the
# source of commit 21286d605815196c3fff1960550ab08700fc4e6f and version
# 2.35.4. The nixpkgs channel stays nixos-26.05. NixOS 26.11 is not
# released.
#
# The system package is nix-cli, not nix-everything. nix-everything's
# installPhase symlinks doc and man to nix-manual (the HTML manual;
# json-schema-for-humans). Its doCheck pulls nix-functional-tests, and
# those tests' nativeBuildInputs include mercurial (python3.13-pip).
# The mail host runs nix-daemon from nix-cli, including the systemd
# units that component installs. It does not run the HTML manual or
# the functional tests. nix-nswrapper is only the non-root daemon
# ExecStart; the default daemon user is root, and this host does not
# change that.

let
  # Mail-host package set only. flake nixosConfigurations.mail-vps
  # appends this. mkPkgs (checks.x86_64-linux.ci, devShells) does not.
  tzdataOverlay = _final: prev: {
    tzdata = prev.tzdata.overrideAttrs (_old: {
      doCheck = false; # https://lists.iana.org/hyperkitty/list/tz@iana.org/thread/CBEK3HXYBOXGD34ZFDSF64QIDT7ZWGBB/
    });
  };

  pinsOverlay = final: prev: {
    nixVersions = prev.nixVersions.extend (
      _vfinal: vprev:
      let
        # nix-prefetch-url --unpack printed
        # 03h0j2bk97xgm15x02ccph4h1rwwci37mkdy6d1200jxzf51b2a4
        # builtins.convertHash { toHashFormat = "sri"; hashAlgo = "sha256"; }
        # printed sha256-RIkVivtdAiBCM77NekZknOcACbyMCdBLqK+fNJeQAA4=
        nixSrc2354 = prev.fetchFromGitHub {
          owner = "NixOS";
          repo = "nix";
          rev = "21286d605815196c3fff1960550ab08700fc4e6f";
          hash = "sha256-RIkVivtdAiBCM77NekZknOcACbyMCdBLqK+fNJeQAA4=";
        };
        components = vprev.nixComponents_2_35.overrideScope (
          _finalScope: _prevScope: {
            version = "2.35.4";
            src = nixSrc2354;
            patchedSrc = nixSrc2354;
          }
        );
      in
      {
        nixComponents_2_35 = components;
        # nix-cli: binaries and systemd units. Not nix-everything
        # (that package builds nix-manual and mercurial).
        nix_2_35 = components.nix-cli;
        latest = components.nix-cli;
      }
    );

    # Channel stable stays the 2.34.9 nix-everything attribute.
    # Nothing in the mail-host module list selects it. The package
    # the system runs is pkgs.nix, the 2.35.4 nix-cli above.
    nix = final.nixVersions.nix_2_35;
  };
in
{
  pins = pinsOverlay;
  tzdata = tzdataOverlay;
}
