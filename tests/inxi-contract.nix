# Targeted inxi host-package contract (not the 14-minute module-eval suite).
# Impure flake pin so `just test-inxi-eval` can nix-eval this file.
# Never embeds host SKUs, addresses, or RAM/swap sizes.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalInxi =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/inxi.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "inxi-eval";
          fileSystems."/" = {
            device = "nodev";
            fsType = "ext4";
          };
          boot.loader.grub.enable = false;
          surmount = extra // {
            enable = extra.enable or true;
          };
        }
      ];
    };
  isInxi =
    p:
    (p.pname or "") == "inxi"
    || lib.hasPrefix "inxi-" (p.name or "")
    || lib.hasSuffix "-inxi" (p.name or "");

  t41-inxi-system-package =
    let
      e = evalInxi { };
      pkgsList = e.config.environment.systemPackages;
    in
    assert builtins.any isInxi pkgsList;
    "t41-inxi-system-package-ok";

  t41b-inxi-off-when-surmount-disabled =
    let
      e = evalInxi { enable = false; };
      pkgsList = e.config.environment.systemPackages;
    in
    assert !(builtins.any isInxi pkgsList);
    "t41b-inxi-off-when-surmount-disabled-ok";
in
{
  t41-inxi-system-package = t41-inxi-system-package;
  t41b-inxi-off-when-surmount-disabled = t41b-inxi-off-when-surmount-disabled;
}
