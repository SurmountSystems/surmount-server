# Gate for a Stalwart store cutover. Default is off.
#
# surmount.mailDataDir is the existing data directory (modules/options.nix).
# modules/mail.nix copies it to services.stalwart.dataDir. This module does
# not set either option, services.stalwart.storePath, or the stalwart-mail
# ExecStart. A failed migrated directory does not become the live server
# through this module. acceptSkippedObjects only suppresses the eval throw.
# It does not retarget the running service.

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount.stalwartStoreMigrate;
in
{
  options.surmount.stalwartStoreMigrate = {
    cutover = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        When true, evaluation throws unless acceptSkippedObjects is also
        true. Default false. This does not change surmount.mailDataDir or
        services.stalwart.dataDir. The live store stays the existing
        Stalwart data directory.
      '';
    };

    acceptSkippedObjects = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Explicit accept required before cutover can evaluate. The copy-side
        report had errors. Setting this true does not retarget the store
        and does not change the running service data path.
      '';
    };
  };

  # NixOS eval throws from system.build.toplevel via checkAssertWarn,
  # which forces each assertion. The throw is inside that thunk so module
  # collection does not fire it when cutover is off. This config does not
  # assign surmount.mailDataDir, services.stalwart.dataDir, storePath,
  # or the stalwart-mail ExecStart.
  config.assertions = [
    {
      assertion =
        if cfg.cutover && !cfg.acceptSkippedObjects then
          throw ''
            surmount.stalwartStoreMigrate.cutover refuses to evaluate unless
            surmount.stalwartStoreMigrate.acceptSkippedObjects is true.
            The copy-side report had errors. This module does not point the
            store at a migrated or backup directory.
          ''
        else
          true;
      message = ''
        surmount.stalwartStoreMigrate.cutover refuses to evaluate unless
        surmount.stalwartStoreMigrate.acceptSkippedObjects is true.
        The copy-side report had errors. This module does not point the
        store at a migrated or backup directory.
      '';
    }
  ];
}
