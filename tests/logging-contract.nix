# Targeted host paper-trail logging contracts (not the 14-minute module-eval suite).
# Impure flake pin so `just test-logging-eval` can nix-eval this file.
# Never embeds host SKUs, addresses, RAM, or guest disk sizes.
# Named contract: persistent journald with a size cap when logging is on;
# sshd VERBOSE auth/disconnects; nixbuilder can read the system journal.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  evalLogging =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/logging.nix
        ../modules/hardening.nix
        ../modules/remote-builder.nix
        ../modules/lake.nix
        {
          system.stateVersion = "26.05";
          networking.hostName = "logging-eval";
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
  failedAssertions = e: builtins.filter (a: !a.assertion) e.config.assertions;
  journalExtra = e: e.config.services.journald.extraConfig or "";
  hasLine =
    text: needle:
    builtins.match (".*" + needle + ".*") (builtins.replaceStrings [ "\n" ] [ " " ] text) != null;

  t42-journal-persistent-when-logging-on =
    let
      e = evalLogging { };
      extra = journalExtra e;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.logging.enable == true;
    assert hasLine extra "Storage=persistent";
    assert hasLine extra "SystemMaxUse=";
    assert hasLine extra "RuntimeMaxUse=";
    assert hasLine extra "RateLimitIntervalSec=";
    assert hasLine extra "RateLimitBurst=";
    assert hasLine extra "ForwardToSyslog=no";
    "t42-journal-persistent-when-logging-on-ok";

  t42b-system-max-use-is-scaffold-not-sku =
    let
      e = evalLogging { };
      extra = journalExtra e;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.logging.systemMaxUse == "1G";
    assert hasLine extra "SystemMaxUse=1G";
    assert e.config.surmount.logging.runtimeMaxUse == "256M";
    assert hasLine extra "RuntimeMaxUse=256M";
    assert e.config.surmount.logging.rateLimitIntervalSec == "30s";
    assert e.config.surmount.logging.rateLimitBurst == "50000";
    assert hasLine extra "RateLimitIntervalSec=30s";
    assert hasLine extra "RateLimitBurst=50000";
    "t42b-system-max-use-is-scaffold-not-sku-ok";

  t42c-logging-off-skips-persistent-journal =
    let
      e = evalLogging { logging.enable = false; };
      extra = journalExtra e;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.logging.enable == false;
    assert !(hasLine extra "Storage=persistent");
    assert !(hasLine extra "SystemMaxUse=");
    assert !(hasLine extra "RateLimitIntervalSec=");
    assert !(hasLine extra "RateLimitBurst=");
    "t42c-logging-off-skips-persistent-journal-ok";

  t42d-sshd-verbose-when-logging-on =
    let
      e = evalLogging { };
      level = e.config.services.openssh.settings.LogLevel or "";
    in
    assert failedAssertions e == [ ];
    assert level == "VERBOSE";
    "t42d-sshd-verbose-when-logging-on-ok";

  t42e-nixbuilder-reads-system-journal =
    let
      e = evalLogging { remoteBuilder.enable = true; };
      extraGroups = e.config.users.users.nixbuilder.extraGroups or [ ];
    in
    assert failedAssertions e == [ ];
    assert builtins.elem "systemd-journal" extraGroups;
    "t42e-nixbuilder-reads-system-journal-ok";

  t42f-nixbuilder-no-journal-group-when-logging-off =
    let
      e = evalLogging {
        logging.enable = false;
        remoteBuilder.enable = true;
      };
      extraGroups = e.config.users.users.nixbuilder.extraGroups or [ ];
    in
    assert failedAssertions e == [ ];
    assert !(builtins.elem "systemd-journal" extraGroups);
    "t42f-nixbuilder-no-journal-group-when-logging-off-ok";

  t42g-zero-rate-limit-interval-fails-closed =
    let
      e = evalLogging { logging.rateLimitIntervalSec = "0s"; };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    "t42g-zero-rate-limit-interval-fails-closed-ok";

  t42h-zero-rate-limit-burst-fails-closed =
    let
      e = evalLogging { logging.rateLimitBurst = "0"; };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    "t42h-zero-rate-limit-burst-fails-closed-ok";

  # Completeness over vacuum: do not shrink burst below 20000.
  t42i-low-rate-limit-burst-fails-closed =
    let
      e = evalLogging { logging.rateLimitBurst = "10000"; };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    "t42i-low-rate-limit-burst-fails-closed-ok";

  t42j-nix-daemon-log-burst-not-tiny =
    let
      e = evalLogging { remoteBuilder.enable = true; };
      d = e.config.systemd.services.nix-daemon.serviceConfig or { };
    in
    assert failedAssertions e == [ ];
    assert (d.LogRateLimitBurst or 0) >= 20000;
    "t42j-nix-daemon-log-burst-not-tiny-ok";
in
{
  t42-journal-persistent-when-logging-on = t42-journal-persistent-when-logging-on;
  t42b-system-max-use-is-scaffold-not-sku = t42b-system-max-use-is-scaffold-not-sku;
  t42c-logging-off-skips-persistent-journal = t42c-logging-off-skips-persistent-journal;
  t42d-sshd-verbose-when-logging-on = t42d-sshd-verbose-when-logging-on;
  t42e-nixbuilder-reads-system-journal = t42e-nixbuilder-reads-system-journal;
  t42f-nixbuilder-no-journal-group-when-logging-off =
    t42f-nixbuilder-no-journal-group-when-logging-off;
  t42g-zero-rate-limit-interval-fails-closed = t42g-zero-rate-limit-interval-fails-closed;
  t42h-zero-rate-limit-burst-fails-closed = t42h-zero-rate-limit-burst-fails-closed;
  t42i-low-rate-limit-burst-fails-closed = t42i-low-rate-limit-burst-fails-closed;
  t42j-nix-daemon-log-burst-not-tiny = t42j-nix-daemon-log-burst-not-tiny;
}
