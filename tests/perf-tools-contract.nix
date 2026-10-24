# Targeted fio host-package contract (not the 14-minute module-eval suite).
# Impure flake pin so `just test-perf-tools-eval` can nix-eval this file.
# Never embeds host SKUs, addresses, or RAM/swap sizes.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalPerf =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/perf-tools.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "perf-tools-eval";
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
  isFio =
    p:
    (p.pname or "") == "fio"
    || lib.hasPrefix "fio-" (p.name or "")
    || lib.hasSuffix "-fio" (p.name or "");

  t-fio-system-package =
    let
      e = evalPerf { };
      pkgsList = e.config.environment.systemPackages;
    in
    assert builtins.any isFio pkgsList;
    "t-fio-system-package-ok";

  t-fio-off-when-surmount-disabled =
    let
      e = evalPerf { enable = false; };
      pkgsList = e.config.environment.systemPackages;
    in
    assert !(builtins.any isFio pkgsList);
    "t-fio-off-when-surmount-disabled-ok";

  t-fio-nixpkgs-pname =
    let
      pkgs = flake.inputs.nixpkgs.legacyPackages.${system};
    in
    assert pkgs.fio.pname == "fio";
    "t-fio-nixpkgs-pname-ok";
in
{
  t-fio-system-package = t-fio-system-package;
  t-fio-off-when-surmount-disabled = t-fio-off-when-surmount-disabled;
  t-fio-nixpkgs-pname = t-fio-nixpkgs-pname;
}
