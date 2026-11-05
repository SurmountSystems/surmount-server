# Optional overlays. The flake injects Surmount packages via surmountOverlay
# in flake.nix (stalwart-mail, stalwart-cli, management-ui, artiOnionService,
# ops bins: private-data / host-logs / shc / niced-builder / leftover-homes /
# host-probe / static-sites / diskstation / deploy-host / dns-zone /
# domain-audit / host-cutover / acme-namecheap / stalwart-ops / mail-import /
# secrets-install, grok-oss, splora / splora-liquid from flake input packages).
# Keep this file for any extra pins that should not live in the flake body.
#
# artiOnionService lives in flake surmountOverlay (distinct from stock
# pkgs.arti). Surmount-owned Arti 2.5.1 source build + onion-service-service;
# not a feature-only override of channel 1.4.2. See
# nix/packages/arti-onion-service.nix.
#
# tzdata 2026e: now.ck is date-dependent. On or after 2026-11-01 the check
# fails because America/Phoenix and America/Vancouver are identical from
# now on (America/Edmonton and America/Winnipeg are the same class). This
# is not a gcc or x86-64-v3 failure. Same attribute change as
# NixOS/nixpkgs pull 582069. gcc.arch and x86-64-v3 stay as they are.

_final: prev: {
  tzdata = prev.tzdata.overrideAttrs (_old: {
    doCheck = false; # https://lists.iana.org/hyperkitty/list/tz@iana.org/thread/CBEK3HXYBOXGD34ZFDSF64QIDT7ZWGBB/
  });
}
