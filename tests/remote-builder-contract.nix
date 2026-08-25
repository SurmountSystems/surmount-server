# Targeted remote-builder contracts (not the 14-minute module-eval suite).
# Impure flake pin so `just test-remote-builder-eval` can nix-eval this file.
# Never embeds host SKUs, addresses, or MemoryMax guest sizes.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalRB =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/remote-builder.nix
        {
          nixpkgs.overlays = [
            (_final: prev: {
              surmount-niced-builder = prev.writeShellScriptBin "surmount-niced-builder" "exit 0";
            })
          ];
          system.stateVersion = "26.05";
          networking.hostName = "rb-eval";
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
  daemonCfg =
    e:
    let
      svc = e.config.systemd.services.nix-daemon or { };
    in
    svc.serviceConfig or { };
  sliceCfg = e: name: e.config.systemd.slices.${name}.sliceConfig or { };

  # rustc on ssh-ng is forwarded to system nix-daemon. MemoryMax must be
  # on that service, not only on the nixbuilder user slice.
  t39g-nix-daemon-is-the-real-build-cgroup =
    let
      e = evalRB { remoteBuilder.enable = true; };
      d = daemonCfg e;
    in
    assert failedAssertions e == [ ];
    assert d.MemoryMax or null == e.config.surmount.remoteBuilder.memoryMax;
    assert d.Nice or null == 19;
    assert (d.IOSchedulingClass or "") == "idle";
    assert (d.MemoryAccounting or false) == true;
    "t39g-nix-daemon-is-the-real-build-cgroup-ok";

  # Job slots are memory-safe. Do not default to a fake 64-job advert.
  t39h-max-jobs-tracks-cores-not-fake-64 =
    let
      e = evalRB { remoteBuilder.enable = true; };
      jobs = e.config.nix.settings.max-jobs;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.remoteBuilder.maxJobs == 8;
    assert jobs == 8;
    assert jobs != 64;
    "t39h-max-jobs-tracks-cores-not-fake-64-ok";

  t39i-custom-max-jobs-not-sku =
    let
      e = evalRB {
        remoteBuilder.enable = true;
        remoteBuilder.maxJobs = 4;
      };
    in
    assert failedAssertions e == [ ];
    assert e.config.nix.settings.max-jobs == 4;
    "t39i-custom-max-jobs-not-sku-ok";

  # auto CPUQuota must not pin the builder path to 95 percent of one CPU.
  t39j-cpu-quota-auto-not-one-cpu =
    let
      e = evalRB { remoteBuilder.enable = true; };
      slice = sliceCfg e "surmount-builder";
      userSliceName = "user-${toString e.config.users.users.nixbuilder.uid}";
      userSlice = sliceCfg e userSliceName;
      d = daemonCfg e;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.remoteBuilder.cpuQuota == "auto";
    assert e.config.surmount.remoteBuilder.onlineCpus == null;
    assert !(slice ? CPUQuota);
    assert !(userSlice ? CPUQuota);
    assert !(d ? CPUQuota);
    "t39j-cpu-quota-auto-not-one-cpu-ok";

  # auto + measured onlineCpus is 95 percent of those CPUs, not 95 percent
  # of one CPU. Do not bake the live guest count into this test.
  t39p-online-cpus-sets-multi-cpu-quota =
    let
      e = evalRB {
        remoteBuilder.enable = true;
        remoteBuilder.onlineCpus = 8;
      };
      slice = sliceCfg e "surmount-builder";
      d = daemonCfg e;
    in
    assert failedAssertions e == [ ];
    assert slice.CPUQuota or null == "760%";
    assert d.CPUQuota or null == "760%";
    assert (d.LogRateLimitBurst or 0) >= 20000;
    "t39p-online-cpus-sets-multi-cpu-quota-ok";

  t39k-explicit-cpu-quota-still-honored =
    let
      e = evalRB {
        remoteBuilder.enable = true;
        remoteBuilder.cpuQuota = "190%";
      };
      slice = sliceCfg e "surmount-builder";
    in
    assert failedAssertions e == [ ];
    assert slice.CPUQuota or null == "190%";
    "t39k-explicit-cpu-quota-still-honored-ok";

  t39l-mail-not-niced-when-builder-on =
    let
      e = evalRB { remoteBuilder.enable = true; };
      mail = e.config.systemd.services.stalwart-mail.serviceConfig or { };
      sshd = e.config.systemd.services.sshd.serviceConfig or { };
    in
    assert failedAssertions e == [ ];
    assert !(mail ? Nice);
    assert !(sshd ? Nice);
    assert !(e.config.systemd.services ? surmount-lake);
    "t39l-mail-not-niced-when-builder-on-ok";

  featureList =
    v:
    if v == null then
      [ ]
    else if builtins.isList v then
      v
    else
      lib.splitString " " v;

  # ssh-ng client advertises surmount-remote; the builder nix-daemon must
  # list it too or rustc is not eligible on this machine. Use extra- so
  # NixOS auto-detected features (big-parallel) stay. grok-build
  # just check-remote / require_remote_builder gates on this name in the
  # live daemon system-features list.
  t39m-extra-system-features-advertises-surmount-remote =
    let
      e = evalRB { remoteBuilder.enable = true; };
      extra = featureList (e.config.nix.settings.extra-system-features or [ ]);
      sys = featureList (e.config.nix.settings.system-features or [ ]);
    in
    assert failedAssertions e == [ ];
    assert lib.elem "surmount-remote" extra;
    assert !(lib.elem "surmount-remote" sys) || lib.elem "surmount-remote" extra;
    "t39m-extra-system-features-advertises-surmount-remote-ok";

  # Default-off must not require the grok-build machines-file feature.
  t39n-disabled-does-not-require-surmount-remote =
    let
      e = evalRB { remoteBuilder.enable = false; };
      extra = featureList (e.config.nix.settings.extra-system-features or [ ]);
      sys = featureList (e.config.nix.settings.system-features or [ ]);
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.remoteBuilder.enable == false;
    assert !(lib.elem "surmount-remote" extra);
    assert !(lib.elem "surmount-remote" sys);
    "t39n-disabled-does-not-require-surmount-remote-ok";

  # SHC 261/262: builder is the preferred in-guest OOM victim. Mail and
  # sshd stay un-niced and less likely to be killed than nix-daemon.
  t39o-builder-is-preferred-oom-victim =
    let
      e = evalRB { remoteBuilder.enable = true; };
      d = daemonCfg e;
      mail = e.config.systemd.services.stalwart-mail.serviceConfig or { };
      sshd = e.config.systemd.services.sshd.serviceConfig or { };
      ui = e.config.systemd.services.surmount-management-ui.serviceConfig or { };
    in
    assert failedAssertions e == [ ];
    assert (d.OOMScoreAdjust or 0) > 0;
    assert (mail.OOMScoreAdjust or 0) < 0;
    assert (sshd.OOMScoreAdjust or 0) < 0;
    assert (ui.OOMScoreAdjust or 0) < 0;
    assert !(mail ? Nice);
    assert !(sshd ? Nice);
    "t39o-builder-is-preferred-oom-victim-ok";
in
{
  ok = [
    t39g-nix-daemon-is-the-real-build-cgroup
    t39h-max-jobs-tracks-cores-not-fake-64
    t39i-custom-max-jobs-not-sku
    t39j-cpu-quota-auto-not-one-cpu
    t39p-online-cpus-sets-multi-cpu-quota
    t39k-explicit-cpu-quota-still-honored
    t39l-mail-not-niced-when-builder-on
    t39m-extra-system-features-advertises-surmount-remote
    t39n-disabled-does-not-require-surmount-remote
    t39o-builder-is-preferred-oom-victim
  ];
}
