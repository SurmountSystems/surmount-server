# Hermetic contract: sshd fail2ban ban is permanent after 3 failures,
# and the ban action is the one this host's firewall applies.
# Host firewall is modules/networking.nix: networking.firewall on,
# networking.nftables.enable left false, so the backend is iptables.
# Do not enable the management UI nft helper. Do not add jails.
# Never embeds host SKUs, addresses, RAM, or guest disk sizes.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalHardening =
    extraModules:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/hardening.nix
        ../modules/networking.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "fail2ban-permanent-eval";
          fileSystems."/" = {
            device = "nodev";
            fsType = "ext4";
          };
          boot.loader.grub.enable = false;
          surmount = {
            enable = true;
            hardening.enable = true;
          };
        }
      ]
      ++ extraModules;
    };
  failedAssertions = e: builtins.filter (a: !a.assertion) e.config.assertions;

  # Product jail is sshd. DEFAULT is the fail2ban defaults section the
  # NixOS module always writes (bantime, maxretry, banaction). It is
  # not a second jail we added.
  wantJails = [
    "DEFAULT"
    "sshd"
  ];

  checkHostFirewall =
    let
      e = evalHardening [ ];
      f2b = e.config.services.fail2ban;
      defs = f2b.jails.DEFAULT.settings;
      sshd = f2b.jails.sshd.settings;
      fw = e.config.networking.firewall;
      jailNames = builtins.attrNames f2b.jails;
    in
    assert failedAssertions e == [ ];
    assert lib.assertMsg (f2b.maxretry == 3) "maxretry is ${toString f2b.maxretry}, want 3";
    assert lib.assertMsg (f2b.bantime == "-1") "bantime is ${f2b.bantime}, want -1 (permanent)";
    assert lib.assertMsg (defs.maxretry == 3) "DEFAULT maxretry is ${toString defs.maxretry}, want 3";
    assert lib.assertMsg (
      defs.bantime == "-1"
    ) "DEFAULT bantime is ${defs.bantime}, want -1 (permanent)";
    assert lib.assertMsg (!(sshd ? maxretry)) "sshd jail must inherit maxretry, not override it";
    assert lib.assertMsg (!(sshd ? bantime)) "sshd jail must inherit bantime, not override it";
    assert lib.assertMsg (f2b.enable == true) "fail2ban must be enabled";
    assert lib.assertMsg (sshd.enabled == true) "sshd jail must be enabled";
    assert lib.assertMsg (sshd.filter == "sshd") "sshd jail filter must be sshd";
    assert lib.assertMsg (sshd.backend == "systemd") "sshd jail backend must be systemd";
    assert lib.assertMsg (
      jailNames == wantJails
    ) "jails are ${builtins.concatStringsSep "," jailNames}, want DEFAULT,sshd";
    assert lib.assertMsg (fw.enable == true) "host firewall must be enabled";
    assert lib.assertMsg (
      e.config.networking.nftables.enable == false
    ) "host firewall must not turn on native nftables";
    assert lib.assertMsg (fw.backend == "iptables") "firewall backend is ${fw.backend}, want iptables";
    assert lib.assertMsg (
      f2b.banaction == "iptables-multiport"
    ) "banaction is ${f2b.banaction}, want iptables-multiport";
    assert lib.assertMsg (
      defs.banaction == "iptables-multiport"
    ) "DEFAULT banaction is ${defs.banaction}, want iptables-multiport";
    assert lib.assertMsg (!(sshd ? action)) "sshd jail must not override action";
    assert lib.assertMsg (!(sshd ? banaction)) "sshd jail must not override banaction";
    assert lib.assertMsg (
      f2b.packageFirewall == fw.package
    ) "fail2ban firewall package must be the host firewall package";
    assert lib.assertMsg (
      fw.package.pname == "iptables"
    ) "firewall package is ${fw.package.pname}, want iptables";
    assert lib.assertMsg (builtins.elem "firewall.service" e.config.systemd.services.fail2ban.partOf)
      "fail2ban must be part of the host firewall unit";
    assert lib.assertMsg (f2b.bantime-increment.enable == false) "bantime increment must stay off";
    assert lib.assertMsg (
      (f2b.daemonSettings.Definition.dbfile or "") == "/var/lib/fail2ban/fail2ban.sqlite3"
    ) "permanent bans need the fail2ban sqlite db";
    assert lib.assertMsg (e.config.surmount.accessControl.nftHelper == false) "nftHelper must stay off";
    assert lib.assertMsg (
      e.config.surmount.accessControl.enable == false
    ) "accessControl must stay off";
    "host-firewall-permanent-sshd-ban-ok";

  # If native nftables is turned on, ip_tables is blacklisted. The ban
  # action must switch to nftables-multiport or the ban never installs.
  # This check forces that in the eval only. It does not enable nftHelper
  # and it does not add a jail.
  checkNftablesSwitch =
    let
      e = evalHardening [
        { networking.nftables.enable = true; }
      ];
      f2b = e.config.services.fail2ban;
      fw = e.config.networking.firewall;
    in
    assert failedAssertions e == [ ];
    assert lib.assertMsg (
      f2b.maxretry == 3
    ) "nftables-switch maxretry is ${toString f2b.maxretry}, want 3";
    assert lib.assertMsg (f2b.bantime == "-1") "nftables-switch bantime is ${f2b.bantime}, want -1";
    assert lib.assertMsg (
      e.config.networking.nftables.enable == true
    ) "nftables switch eval must enable nftables";
    assert lib.assertMsg (
      fw.backend == "nftables"
    ) "nftables switch backend is ${fw.backend}, want nftables";
    assert lib.assertMsg (
      f2b.banaction == "nftables-multiport"
    ) "nftables switch banaction is ${f2b.banaction}, want nftables-multiport";
    assert lib.assertMsg (
      f2b.jails.DEFAULT.settings.banaction == "nftables-multiport"
    ) "nftables switch DEFAULT banaction drifted";
    assert lib.assertMsg (!(f2b.jails.sshd.settings ? action)) "sshd jail must not pin an action";
    assert lib.assertMsg (
      f2b.packageFirewall == fw.package
    ) "nftables switch packageFirewall must match the firewall package";
    assert lib.assertMsg (
      fw.package.pname == "nftables"
    ) "nftables switch package is ${fw.package.pname}, want nftables";
    assert lib.assertMsg (builtins.elem "ip_tables" e.config.boot.blacklistedKernelModules)
      "nftables firewall must blacklist ip_tables, so an iptables ban action cannot install";
    assert lib.assertMsg (
      e.config.surmount.accessControl.nftHelper == false
    ) "nftHelper must stay off when nftables is on";
    assert lib.assertMsg (
      builtins.attrNames f2b.jails == wantJails
    ) "nftables switch must not add jails";
    "nftables-switch-permanent-sshd-ban-ok";
in
{
  host-firewall-permanent-sshd-ban = checkHostFirewall;
  nftables-switch-permanent-sshd-ban = checkNftablesSwitch;
}
