# Targeted Lake module contracts (not the 14-minute module-eval suite).
# Impure flake pin so `nix eval --impure --json --file tests/lake-contract.nix`
# can run this file. Never embeds host SKUs, addresses, or guest RAM sizes.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  pkgs = flake.inputs.nixpkgs.legacyPackages.${system};
  fakeLake = pkgs.writeShellScriptBin "lake" "exit 0";
  evalLake =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/lake.nix
        ../modules/logging.nix
        ../modules/remote-builder.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "lake-eval";
          fileSystems."/" = {
            device = "nodev";
            fsType = "ext4";
          };
          boot.loader.grub.enable = false;
          surmount = extra // {
            enable = true;
          };
        }
      ];
    };
  failedAssertions = e: builtins.filter (a: !a.assertion) e.config.assertions;
  svcCfg = e: name: (e.config.systemd.services.${name} or { }).serviceConfig or { };
  sliceCfg = e: name: e.config.systemd.slices.${name}.sliceConfig or { };

  t44-lake-default-off-no-unit =
    let
      e = evalLake { };
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.lake.enable == false;
    assert e.config.surmount.lake.jobs == 4;
    assert e.config.surmount.lake.jobs != 64;
    assert e.config.surmount.lake.memoryMax == "4G";
    assert e.config.surmount.lake.cpuQuota == "auto";
    assert !(e.config.systemd.services ? surmount-lake);
    assert !(e.config.systemd.slices ? "surmount-lake");
    assert !(e.config.programs.nix-ld.enable or false);
    "t44-lake-default-off-no-unit-ok";

  t44b-lake-enable-caps-the-real-unit =
    let
      e = evalLake {
        lake.enable = true;
        lake.package = fakeLake;
      };
      s = svcCfg e "surmount-lake";
      slice = sliceCfg e "surmount-lake";
      mail = svcCfg e "stalwart-mail";
      sshd = svcCfg e "sshd";
    in
    assert failedAssertions e == [ ];
    assert e.config.systemd.services ? surmount-lake;
    assert builtins.elem "multi-user.target" (e.config.systemd.services.surmount-lake.wantedBy or [ ]);
    assert s.MemoryMax or null == e.config.surmount.lake.memoryMax;
    assert s.Nice or null == 19;
    assert (s.IOSchedulingClass or "") == "idle";
    assert (s.OOMScoreAdjust or 0) > 0;
    assert (s.LogRateLimitBurst or 0) >= 20000;
    assert (s.SyslogIdentifier or "") == "surmount-lake";
    assert slice.MemoryMax or null == "4G";
    assert !(slice ? CPUQuota);
    assert !(s ? CPUQuota);
    assert lib.hasInfix "-j4" (s.ExecStart or "");
    assert !(mail ? Nice);
    assert !(sshd ? Nice);
    "t44b-lake-enable-caps-the-real-unit-ok";

  t44c-lake-empty-memory-max-asserts =
    let
      e = evalLake {
        lake.enable = true;
        lake.package = fakeLake;
        lake.memoryMax = "";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    "t44c-lake-empty-memory-max-asserts-ok";

  t44d-lake-enable-requires-package =
    let
      e = evalLake { lake.enable = true; };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    "t44d-lake-enable-requires-package-ok";

  t44e-lake-online-cpus-multi-cpu-quota =
    let
      e = evalLake {
        lake.enable = true;
        lake.package = fakeLake;
        lake.onlineCpus = 8;
      };
      s = svcCfg e "surmount-lake";
      slice = sliceCfg e "surmount-lake";
    in
    assert failedAssertions e == [ ];
    assert s.CPUQuota or null == "760%";
    assert slice.CPUQuota or null == "760%";
    "t44e-lake-online-cpus-multi-cpu-quota-ok";

  t44f-overlay-lake-without-option-fails =
    let
      e = lib.nixosSystem {
        inherit system;
        modules = [
          ../modules/options.nix
          ../modules/lake.nix
          {
            system.stateVersion = "26.05";
            networking.hostName = "lake-overlay-eval";
            fileSystems."/" = {
              device = "nodev";
              fsType = "ext4";
            };
            boot.loader.grub.enable = false;
            surmount.enable = true;
            systemd.services.surmount-lake.wantedBy = lib.mkForce [ "multi-user.target" ];
          }
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "surmount-lake" a.message) failed;
    "t44f-overlay-lake-without-option-fails-ok";

  t44g-lake-user-reads-journal-when-logging-on =
    let
      e = evalLake {
        lake.enable = true;
        lake.package = fakeLake;
      };
      extraGroups = e.config.users.users.surmount-lake.extraGroups or [ ];
    in
    assert failedAssertions e == [ ];
    assert builtins.elem "systemd-journal" extraGroups;
    "t44g-lake-user-reads-journal-when-logging-on-ok";
in
{
  t44-lake-default-off-no-unit = t44-lake-default-off-no-unit;
  t44b-lake-enable-caps-the-real-unit = t44b-lake-enable-caps-the-real-unit;
  t44c-lake-empty-memory-max-asserts = t44c-lake-empty-memory-max-asserts;
  t44d-lake-enable-requires-package = t44d-lake-enable-requires-package;
  t44e-lake-online-cpus-multi-cpu-quota = t44e-lake-online-cpus-multi-cpu-quota;
  t44f-overlay-lake-without-option-fails = t44f-overlay-lake-without-option-fails;
  t44g-lake-user-reads-journal-when-logging-on = t44g-lake-user-reads-journal-when-logging-on;
}
