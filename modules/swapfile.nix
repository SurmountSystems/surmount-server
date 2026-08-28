# Optional 256 GiB swap file. Path is host-local (disk). Refuse if enable
# and path empty. Scram still keys off RAM, not swap fill.

{
  config,
  lib,
  ...
}:
let
  inherit (lib) mkIf;
  sf = config.surmount.swapFile;
in
{
  config = mkIf (config.surmount.enable && sf.enable) {
    assertions = [
      {
        assertion = sf.path != "";
        message = "surmount.swapFile.enable requires surmount.swapFile.path (host-local disk). Do not invent a path in the public tree.";
      }
    ];
    swapDevices = lib.optionals (sf.path != "") [
      {
        device = sf.path;
        size = sf.sizeGiB * 1024;
      }
    ];
  };
}
