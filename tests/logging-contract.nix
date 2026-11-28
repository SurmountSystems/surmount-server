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
  # release-26.11 removed services.journald.extraConfig. The same
  # journald.conf [Journal] keys are services.journald.settings.Journal.
  journalSettings = e: e.config.services.journald.settings.Journal;

  t42-journal-persistent-when-logging-on =
    let
      e = evalLogging { };
      journal = journalSettings e;
      log = e.config.surmount.logging;
    in
    assert failedAssertions e == [ ];
    assert log.enable == true;
    # Storage=persistent
    assert journal.Storage == "persistent";
    # SystemMaxUse=<surmount.logging.systemMaxUse>
    assert journal.SystemMaxUse == log.systemMaxUse;
    # RuntimeMaxUse=<surmount.logging.runtimeMaxUse>
    assert journal.RuntimeMaxUse == log.runtimeMaxUse;
    # RateLimitIntervalSec=<surmount.logging.rateLimitIntervalSec>
    assert journal.RateLimitIntervalSec == log.rateLimitIntervalSec;
    # RateLimitBurst=<surmount.logging.rateLimitBurst>
    assert journal.RateLimitBurst == log.rateLimitBurst;
    # ForwardToSyslog=no
    assert journal.ForwardToSyslog == "no";
    "t42-journal-persistent-when-logging-on-ok";

  t42b-system-max-use-is-scaffold-not-sku =
    let
      e = evalLogging { };
      journal = journalSettings e;
      log = e.config.surmount.logging;
    in
    assert failedAssertions e == [ ];
    assert log.systemMaxUse == "1G";
    # SystemMaxUse=1G
    assert journal.SystemMaxUse == "1G";
    assert log.runtimeMaxUse == "256M";
    # RuntimeMaxUse=256M
    assert journal.RuntimeMaxUse == "256M";
    assert log.rateLimitIntervalSec == "30s";
    assert log.rateLimitBurst == "50000";
    # RateLimitIntervalSec=30s
    assert journal.RateLimitIntervalSec == "30s";
    # RateLimitBurst=50000
    assert journal.RateLimitBurst == "50000";
    "t42b-system-max-use-is-scaffold-not-sku-ok";

  t42c-logging-off-skips-persistent-journal =
    let
      e = evalLogging { logging.enable = false; };
      journal = journalSettings e;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.logging.enable == false;
    # extraConfig did not contain Storage=persistent
    assert (journal.Storage or null) != "persistent";
    # extraConfig did not contain SystemMaxUse=
    assert !(journal ? SystemMaxUse);
    # extraConfig did not contain RateLimitIntervalSec=
    assert !(journal ? RateLimitIntervalSec);
    # extraConfig did not contain RateLimitBurst=
    assert !(journal ? RateLimitBurst);
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
