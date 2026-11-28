# Hermetic eval: host Let's Encrypt renewal is a unit on the host when
# the host renewal switch is on. Standalone. Do not import this from
# tests/module-eval.nix.
#
#   nix eval --impure --json --file tests/host-cert-renew-eval.nix
#
# Public module default stays off. This file turns the switch on in one
# eval and leaves it off in the other. example.test names only.
let
  flake = builtins.getFlake (toString ./..);
  system = builtins.currentSystem;
  inherit (flake.inputs.nixpkgs) lib;
  pkgs = flake.inputs.nixpkgs.legacyPackages.${system};
  fakeUi = pkgs.writeShellScriptBin "surmount-management-ui" "exit 0";
  evalHost =
    extra:
    lib.nixosSystem {
      inherit system;
      modules = [
        ../modules/options.nix
        ../modules/management-ui.nix
        {
          nixpkgs.overlays = [
            (_final: prev: {
              surmount-management-ui = fakeUi;
              acme-dns-hook-namecheap = prev.writeShellScriptBin "acme-dns-hook-namecheap" "exit 0";
            })
          ];
          system.stateVersion = "26.05";
          networking.hostName = "cert-renew-eval";
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
  envText =
    e:
    let
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment or [ ];
    in
    if builtins.isList env then lib.concatStringsSep "\n" env else toString env;
  timerWanted =
    e:
    e.config.systemd.timers ? surmount-cert-renew
    && builtins.elem "timers.target" (e.config.systemd.timers.surmount-cert-renew.wantedBy or [ ]);

  # Switch off: no renew unit is wanted. Public default stays off.
  host-cert-renew-off-has-no-wanted-timer =
    let
      e = evalHost {
        managementUi.listenMode = "https";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 443;
        managementUi.tlsCertPath = "/var/lib/surmount/secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/var/lib/surmount/secrets/tls/key.pem";
        managementUi.acme.enable = false;
      };
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.managementUi.acme.enable == false;
    assert !(timerWanted e);
    "host-cert-renew-off-has-no-wanted-timer-ok";

  # Switch on, HTTP-01, Let's Encrypt production. The renew timer is wanted.
  host-cert-renew-on-wants-timer =
    let
      e = evalHost {
        managementUi.listenMode = "https";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 443;
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "0.0.0.0:80";
        managementUi.tlsCertPath = "/var/lib/surmount/secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/var/lib/surmount/secrets/tls/key.pem";
        managementUi.acme.enable = true;
        managementUi.acme.challenge = "http-01";
        managementUi.acme.directory = "https://acme-v02.api.letsencrypt.org/directory";
        managementUi.acme.email = "ops@example.test";
        managementUi.acme.domains = [
          "services.example.test"
          "mail.example.test"
        ];
        managementUi.acme.accountCredentialsPath = "/var/lib/surmount/secrets/acme/account.json";
        managementUi.acme.dnsProvider = "none";
      };
      env = envText e;
      svc = e.config.systemd.services.surmount-cert-renew or { };
      exec = svc.serviceConfig.ExecStart or "";
      script = if builtins.isList exec then lib.concatStringsSep " " exec else toString exec;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.managementUi.acme.enable == true;
    assert e.config.surmount.managementUi.acme.challenge == "http-01";
    assert e.config.surmount.managementUi.acme.dnsProvider == "none";
    assert e.config.surmount.vaultwarden.enable == false;
    assert e.config.surmount.backups.enable == false;
    assert e.config.surmount.lake.enable == false;
    assert e.config.surmount.remoteBuilder.enable == false;
    assert e.config.surmount.accessControl.nftHelper == false;
    assert timerWanted e;
    assert e.config.systemd.services ? surmount-cert-renew;
    assert lib.hasInfix "http-01" env;
    assert lib.hasInfix "https://acme-v02.api.letsencrypt.org/directory" env;
    assert lib.hasInfix "SURMOUNT_ACME_HTTP01_WEBROOT=" env;
    assert !(lib.hasInfix "namecheap" env);
    assert script != "";
    assert lib.hasInfix "surmount-management-ui" (builtins.readFile script);
    "host-cert-renew-on-wants-timer-ok";

  # Nostr is on, the session secret is a host file, and HTTP-01 renew is on.
  # The renew unit must not export nostr auth and must not receive that
  # file. acme-renew loads ACME and TLS only. A successful write copies
  # PEMs for Stalwart and restarts stalwart-mail. status.{primaryDomain}
  # stays on the order: measured 2026-11-03, status.surmount.systems
  # answers on port 80 at the same address as the apex.
  host-cert-renew-nostr-without-session-secret =
    let
      sessionPath = "/var/lib/surmount/secrets/ui/session-secret";
      e = evalHost {
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "0.0.0.0:80";
        managementUi.tlsCertPath = "/var/lib/surmount/secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/var/lib/surmount/secrets/tls/key.pem";
        managementUi.authMode = "nostr";
        managementUi.sessionSecretPath = sessionPath;
        managementUi.acme.enable = true;
        managementUi.acme.challenge = "http-01";
        managementUi.acme.directory = "https://acme-v02.api.letsencrypt.org/directory";
        managementUi.acme.email = "ops@example.test";
        managementUi.acme.domains = [
          "services.example.test"
          "mail.example.test"
        ];
        managementUi.acme.accountCredentialsPath = "/var/lib/surmount/secrets/acme/account.json";
        managementUi.acme.dnsProvider = "none";
      };
      textOf =
        v:
        if v == null then
          ""
        else if builtins.isList v then
          lib.concatStringsSep "\n" (map toString v)
        else
          toString v;
      renewSvc = e.config.systemd.services.surmount-cert-renew or { };
      uiSvc = e.config.systemd.services.surmount-management-ui.serviceConfig or { };
      renewEnv = textOf (renewSvc.serviceConfig.Environment or [ ]);
      uiFile = textOf (uiSvc.EnvironmentFile or null);
      renewFile = textOf (renewSvc.serviceConfig.EnvironmentFile or null);
      stripPlus = s: if lib.hasPrefix "+" s then lib.removePrefix "+" s else s;
      firstToken =
        raw:
        let
          parts = lib.filter (p: p != "") (lib.splitString " " (lib.head (lib.splitString "\n" raw)));
        in
        if parts == [ ] then "" else stripPlus (lib.head parts);
      startPath = firstToken (textOf (renewSvc.serviceConfig.ExecStart or ""));
      postPath = firstToken (textOf (renewSvc.serviceConfig.ExecStartPost or ""));
      startText =
        if startPath != "" && builtins.pathExists startPath then
          builtins.readFile startPath
        else
          "missing-start:${startPath}";
      postText =
        if postPath != "" && builtins.pathExists postPath then
          builtins.readFile postPath
        else
          "missing-post:${postPath}";
      misses = lib.filter (m: m != "") [
        (if lib.hasInfix "SURMOUNT_AUTH_MODE=" renewEnv then "renew-env-has-SURMOUNT_AUTH_MODE" else "")
        (
          if lib.hasInfix "SURMOUNT_SESSION_SECRET=" renewEnv then
            "renew-env-has-SURMOUNT_SESSION_SECRET"
          else
            ""
        )
        (if renewFile != "" then "renew-has-EnvironmentFile" else "")
        (if lib.hasInfix sessionPath uiFile then "" else "ui-missing-session-secret-file")
        (
          if lib.hasInfix "status.surmount.systems" renewEnv then
            ""
          else
            "renew-missing-status.surmount.systems"
        )
        (if lib.hasInfix "services.example.test" renewEnv then "" else "renew-dropped-configured-name")
        (if lib.hasInfix "mail.example.test" renewEnv then "" else "renew-dropped-configured-name-mail")
        (if lib.hasInfix "acme-renew" startText then "" else "start-script-missing-acme-renew")
        (if lib.hasInfix "-m 0640" postText then "" else "restart-script-missing-cert-mode-0640")
        (if lib.hasInfix "-m 0600" postText then "" else "restart-script-missing-key-mode-0600")
        (
          if lib.hasInfix "stalwart-mail:stalwart-mail" postText then
            ""
          else
            "restart-script-missing-stalwart-owner"
        )
        (
          if lib.hasInfix "restart stalwart-mail.service" postText then
            ""
          else
            "restart-script-missing-stalwart-restart"
        )
        (if lib.hasInfix "/mail/tls" postText then "" else "restart-script-missing-mail-tls-dir")
        (
          if
            lib.hasInfix sessionPath renewEnv
            || lib.hasInfix sessionPath startText
            || lib.hasInfix sessionPath postText
          then
            "session-secret-path-leaked-into-renew-unit"
          else
            ""
        )
      ];
    in
    assert failedAssertions e == [ ];
    assert timerWanted e;
    if misses != [ ] then
      throw ("host-cert-renew-nostr-without-session-secret misses: " + lib.concatStringsSep ", " misses)
    else
      "host-cert-renew-nostr-without-session-secret-ok";
in
{
  ok = [
    host-cert-renew-off-has-no-wanted-timer
    host-cert-renew-on-wants-timer
    host-cert-renew-nostr-without-session-secret
  ];
}
