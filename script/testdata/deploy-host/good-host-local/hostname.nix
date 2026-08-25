# Synthetic host-local hostname overlay (placeholder only).
# Operators set the real OS hostname on the machine; never commit real box names.
{ lib, ... }:
{
  networking.hostName = lib.mkForce "example-test-host";
}
