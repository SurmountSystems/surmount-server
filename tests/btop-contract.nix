# Targeted btop.conf contract (not the 14-minute module-eval suite).
# Impure flake pin so `just test-btop-eval` can nix-eval this file.
# Named contract: shipped conf has use_fstab = true so disk usage
# follows /etc/fstab (NixOS fileSystems). Extra disks persist with
# fileSystems, not by listing every mount. This public eval does not
# import private host-local, so /mnt/hdd is absent here. Never embeds
# host SKUs, addresses, or disk capacity.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalBtop =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/btop.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "btop-eval";
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
  confText =
    e:
    let
      entry = e.config.environment.etc."xdg/btop/btop.conf" or null;
    in
    if entry == null then
      ""
    else if entry ? source then
      builtins.readFile entry.source
    else if entry ? text then
      entry.text
    else
      "";

  t37b-btop-use-fstab-true =
    let
      e = evalBtop { };
      text = confText e;
    in
    assert text != "";
    assert lib.hasInfix "use_fstab = true" text;
    assert lib.hasInfix ''disks_filter = ""'' text;
    assert lib.hasInfix "show_disks = true" text;
    assert lib.hasInfix "only_physical = true" text;
    assert !(lib.hasInfix "use_fstab = false" text);
    # Public modules-only eval. Host-local hardware-configuration.nix
    # owns fileSystems."/mnt/hdd"; this file does not import it.
    assert !(e.config.fileSystems ? "/mnt/hdd");
    "t37b-btop-use-fstab-true-ok";
in
{
  t37b-btop-use-fstab-true = t37b-btop-use-fstab-true;
}
