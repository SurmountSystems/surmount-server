# Optional overlays. The flake injects Surmount packages via surmountOverlay
# in flake.nix (stalwart-mail, stalwart-cli, management-ui, artiOnionService).
# Keep this file for any extra pins that should not live in the flake body.
#
# artiOnionService lives in flake surmountOverlay (distinct from stock
# pkgs.arti). Surmount-owned Arti 2.5.0 source build + onion-service-service;
# not a feature-only override of channel 1.4.2. See
# nix/packages/arti-onion-service.nix.

_final: _prev: {
  # Intentionally empty. Prefer flake packages + nixosModule specialArgs
  # over a heavy overlay unless many packages need sharing.
}
