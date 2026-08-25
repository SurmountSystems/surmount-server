# Targeted Eternal Terminal contract (not the full module-eval suite).
# Named contract: when hardening is on, etserver is enabled, TCP 2022 is
# open, the unit is not niced, logging is not silent. Mullvad is not in
# this module. Never embeds host SKUs or addresses.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalEt =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/hardening.nix
        ../modules/eternal-terminal.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "et-eval";
          fileSystems."/" = {
            device = "nodev";
            fsType = "ext4";
          };
          boot.loader.grub.enable = false;
          surmount = lib.recursiveUpdate {
            enable = extra.enable or true;
            hardening.enable = extra.hardening.enable or true;
          } extra;
        }
      ];
    };
  failedAssertions = e: builtins.filter (a: !a.assertion) e.config.assertions;

  t45-et-on-by-default-with-hardening =
    let
      e = evalEt { };
      svc = e.config.systemd.services.eternal-terminal.serviceConfig;
    in
    assert failedAssertions e == [ ];
    assert e.config.services.eternal-terminal.enable == true;
    assert e.config.services.eternal-terminal.port == 2022;
    assert e.config.services.eternal-terminal.silent == false;
    assert builtins.elem 2022 e.config.networking.firewall.allowedTCPPorts;
    assert !(svc ? Nice) || svc.Nice == null;
    "t45-et-on-by-default-with-hardening-ok";

  t45b-et-off-when-disabled =
    let
      e = evalEt { hardening.eternalTerminal.enable = false; };
    in
    assert failedAssertions e == [ ];
    assert e.config.services.eternal-terminal.enable == false;
    assert !(builtins.elem 2022 e.config.networking.firewall.allowedTCPPorts);
    "t45b-et-off-when-disabled-ok";
in
{
  t45-et-on-by-default-with-hardening = t45-et-on-by-default-with-hardening;
  t45b-et-off-when-disabled = t45b-et-off-when-disabled;
}
