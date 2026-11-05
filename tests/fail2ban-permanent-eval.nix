# Hermetic contract: sshd bans are permanent after 3 failures, the
# ban uses the host firewall, and fail2ban is not installed.
# sshguard 2.5.1 scores a normal sshd auth failure at 10. Block and
# blacklist both use >=, so attack_threshold 30 and
# blacklist_threshold 30 are 3 failures and a permanent blacklist.
# Host firewall is modules/networking.nix: networking.firewall on,
# networking.nftables.enable left false, so the pinned nixpkgs module
# selects sshg-fw-ipset (iptables + ipset) and partOf firewall.service.
# Turning nftables on in this eval only switches that backend to
# sshg-fw-nft-sets. Do not enable the management UI nft helper.
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
  pathNames = e: map (p: p.pname or p.name or "") e.config.systemd.services.sshguard.path;
  execText = e: toString e.config.systemd.services.sshguard.serviceConfig.ExecStart;

  checkPermanent =
    e:
    let
      sg = e.config.services.sshguard;
    in
    assert lib.assertMsg (sg.enable == true) "sshguard must be enabled";
    assert lib.assertMsg (sg.services == [ "sshd" ]) "sshguard must watch sshd only";
    assert lib.assertMsg (
      sg.attack_threshold == 30
    ) "attack_threshold is ${toString sg.attack_threshold}, want 30 (3 sshd failures)";
    assert lib.assertMsg (
      sg.blacklist_threshold == 30
    ) "blacklist_threshold is ${toString sg.blacklist_threshold}, want 30 (permanent after 3)";
    assert lib.assertMsg (
      toString sg.blacklist_file == "/var/lib/sshguard/blacklist.db"
    ) "blacklist file is ${toString sg.blacklist_file}";
    assert lib.assertMsg (e.config.services.fail2ban.enable == false) "fail2ban must be disabled";
    assert lib.assertMsg (lib.hasInfix "-a 30" (execText e)) "sshguard command must pass -a 30";
    assert lib.assertMsg (lib.hasInfix "-b 30:/var/lib/sshguard/blacklist.db" (
      execText e
    )) "sshguard command must pass a permanent blacklist of 30";
    assert lib.assertMsg (builtins.elem "firewall.service" e.config.systemd.services.sshguard.partOf)
      "sshguard must be part of the host firewall unit";
    assert lib.assertMsg (e.config.surmount.accessControl.nftHelper == false) "nftHelper must stay off";
    assert lib.assertMsg (
      e.config.surmount.accessControl.enable == false
    ) "accessControl must stay off";
    true;

  checkHostFirewall =
    let
      e = evalHardening [ ];
      fw = e.config.networking.firewall;
      names = pathNames e;
      preStart = e.config.systemd.services.sshguard.preStart or "";
    in
    assert failedAssertions e == [ ];
    assert checkPermanent e;
    assert lib.assertMsg (fw.enable == true) "host firewall must be enabled";
    assert lib.assertMsg (
      e.config.networking.nftables.enable == false
    ) "host firewall must not turn on native nftables";
    assert lib.assertMsg (fw.backend == "iptables") "firewall backend is ${fw.backend}, want iptables";
    assert lib.assertMsg (
      fw.package.pname == "iptables"
    ) "firewall package is ${fw.package.pname}, want iptables";
    assert lib.assertMsg (builtins.elem "iptables" names) "sshguard path must include iptables";
    assert lib.assertMsg (builtins.elem "ipset" names) "sshguard path must include ipset";
    assert lib.assertMsg (
      !(builtins.elem "nftables" names)
    ) "iptables host must not put nftables on the sshguard path";
    assert lib.assertMsg (
      lib.hasInfix "iptables" preStart
      && lib.hasInfix "sshguard4" preStart
      && lib.hasInfix "DROP" preStart
    ) "sshguard must install a host-firewall DROP for sshguard4";
    "host-firewall-permanent-sshd-ban-ok";

  # Native nftables blacklists ip_tables. The pinned sshguard module
  # then puts nftables on the unit path (sshg-fw-nft-sets). Thresholds
  # stay 3 failures and permanent. This does not enable nftHelper.
  checkNftablesSwitch =
    let
      e = evalHardening [
        { networking.nftables.enable = true; }
      ];
      fw = e.config.networking.firewall;
      names = pathNames e;
    in
    assert failedAssertions e == [ ];
    assert checkPermanent e;
    assert lib.assertMsg (
      e.config.networking.nftables.enable == true
    ) "nftables switch eval must enable nftables";
    assert lib.assertMsg (
      fw.backend == "nftables"
    ) "nftables switch backend is ${fw.backend}, want nftables";
    assert lib.assertMsg (
      fw.package.pname == "nftables"
    ) "nftables switch package is ${fw.package.pname}, want nftables";
    assert lib.assertMsg (builtins.elem "nftables" names)
      "nftables host must put nftables on the sshguard path";
    assert lib.assertMsg (
      !(builtins.elem "iptables" names)
    ) "nftables host must not put iptables on the sshguard path";
    assert lib.assertMsg (builtins.elem "ip_tables" e.config.boot.blacklistedKernelModules)
      "nftables firewall must blacklist ip_tables";
    assert lib.assertMsg (
      e.config.services.sshguard.services == [ "sshd" ]
    ) "nftables switch must not add services";
    "nftables-switch-permanent-sshd-ban-ok";
in
{
  host-firewall-permanent-sshd-ban = checkHostFirewall;
  nftables-switch-permanent-sshd-ban = checkNftablesSwitch;
}
