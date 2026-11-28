# Cutover stays off. Cutover without an explicit accept throws at eval.
# Cutover off, and cutover with accept, both leave the existing Stalwart
# data directory and ExecStart unchanged. This module must not point the
# store at a migrated or backup directory.
#
#   nice -n 19 nix-instantiate --eval --strict tests/stalwart-store-migrate.nix
#
# Does not embed host addresses, guest sizes, secrets, or blob hashes.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  pkgs = flake.inputs.nixpkgs.legacyPackages.${system};
  fakeBin = name: pkgs.writeShellScriptBin name "exit 0";

  evalMail =
    extraModules: extraSurmount:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/stalwart-service.nix
        ../modules/mail.nix
        {
          nixpkgs.overlays = [
            (_final: _prev: {
              stalwart-mail = fakeBin "stalwart";
              stalwart-cli = fakeBin "stalwart-cli";
              vandelay = fakeBin "vandelay";
              surmount-mail-import = fakeBin "surmount-mail-import";
              surmount-deploy-host = fakeBin "surmount-deploy-host";
            })
          ];
          system.stateVersion = "26.05";
          networking.hostName = "stalwart-store-migrate-eval";
          fileSystems."/" = {
            device = "nodev";
            fsType = "ext4";
          };
          boot.loader.grub.enable = false;
          surmount = {
            enable = true;
            managementUi.enable = false;
          }
          // extraSurmount;
        }
      ]
      ++ extraModules;
    };

  migrateModule = ../modules/stalwart-store-migrate.nix;
  failedAssertions = e: builtins.filter (a: !a.assertion) e.config.assertions;
  dataDirOf = e: e.config.services.stalwart.dataDir;
  mailDataDirOf = e: e.config.surmount.mailDataDir;
  storePathOf = e: e.config.services.stalwart.storePath;
  execOf = e: e.config.systemd.services.stalwart-mail.serviceConfig.ExecStart;

  # Same store settings mail.nix already applies. No migrate module.
  baseline = evalMail [ ] { };

  # Module present, cutover left off (also the option default).
  off = evalMail [ migrateModule ] { };

  # Same discharge as nixos system.build.toplevel (lib.asserts.checkAssertWarn).
  # Cutover without accept throws inside the assertion thunk.
  discharge =
    e:
    lib.asserts.checkAssertWarn e.config.assertions e.config.warnings (
      builtins.deepSeq (toString (dataDirOf e)) (builtins.deepSeq (toString (execOf e)) true)
    );

  refused = builtins.tryEval (
    discharge (
      evalMail [ migrateModule ] {
        stalwartStoreMigrate.cutover = true;
        stalwartStoreMigrate.acceptSkippedObjects = false;
      }
    )
  );

  # Explicit accept still must not retarget the live store.
  accepted = evalMail [ migrateModule ] {
    stalwartStoreMigrate.cutover = true;
    stalwartStoreMigrate.acceptSkippedObjects = true;
  };

  sameStore =
    e:
    toString (dataDirOf e) == toString (dataDirOf baseline)
    && toString (mailDataDirOf e) == toString (mailDataDirOf baseline)
    && toString (storePathOf e) == toString (storePathOf baseline)
    && toString (execOf e) == toString (execOf baseline);
in
assert refused.success == false;
assert discharge off;
assert discharge accepted;
assert failedAssertions off == [ ];
assert failedAssertions accepted == [ ];
assert off.config.surmount.stalwartStoreMigrate.cutover == false;
assert off.config.surmount.stalwartStoreMigrate.acceptSkippedObjects == false;
assert toString (dataDirOf off) == toString (mailDataDirOf off);
assert sameStore off;
assert sameStore accepted;
assert accepted.config.surmount.stalwartStoreMigrate.cutover == true;
assert accepted.config.surmount.stalwartStoreMigrate.acceptSkippedObjects == true;
{
  ok = "stalwart-store-migrate-ok";
  mailDataDirOption = "surmount.mailDataDir";
  stalwartDataDirOption = "services.stalwart.dataDir";
  dataDir = toString (dataDirOf off);
  storePath = toString (storePathOf off);
  dataDirMatchesMailDataDir = toString (dataDirOf off) == toString (mailDataDirOf off);
  cutoverDefault = off.config.surmount.stalwartStoreMigrate.cutover;
  acceptSkippedObjectsDefault = off.config.surmount.stalwartStoreMigrate.acceptSkippedObjects;
  refusedThrew = !refused.success;
  offMatchesBaseline = sameStore off;
  acceptDoesNotRetarget = sameStore accepted;
}
