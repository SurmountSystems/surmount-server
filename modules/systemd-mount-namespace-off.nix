# The guest kernel rejected the `/` remount under systemd 260.5, so these
# units cannot use a mount namespace until that remount works.
#
# Unit names match the guest paste. The four Splora units are the same
# strings the tree already uses: splora-liquid, splora-mutinynet,
# splora-testnet3, and splora-testnet4.
{
  lib,
  ...
}:
let
  off = lib.mkForce false;
  sandboxOff = {
    ProtectSystem = off;
    ProtectHome = off;
    PrivateTmp = off;
    PrivateDevices = off;
    PrivateMounts = off;
    ProtectKernelTunables = off;
    ProtectKernelModules = off;
    ProtectControlGroups = off;
    ProtectClock = off;
    ProtectHostname = off;
    PrivateIPC = off;
    PrivateNetwork = off;
  };
  units = [
    "systemd-udevd"
    "systemd-timesyncd"
    "nscd"
    "mandb"
    "stalwart-mail"
    "surmount-management-ui"
    "surmount-scram"
    "surmount-arti-hidden-service"
    "splora-liquid"
    "splora-mutinynet"
    "splora-testnet3"
    "splora-testnet4"
  ];
in
{
  systemd.services = lib.genAttrs units (_name: {
    serviceConfig = sandboxOff;
  });
}
