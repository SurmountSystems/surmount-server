# Real NixOS module evaluation for Surmount secrets + Arti + management-ui
# options/assertions. Uses lightweight fake packages (no Stalwart FOD).
#
# Call from flake:
#   import ./tests/module-eval.nix { inherit pkgs lib; sops-nix = ...; }
#
# Returns { ok = [ "t..." ... ]; } or throws on failed assert.
# This is the single full nixosSystem contract suite for CI.

{
  pkgs,
  lib,
  sops-nix,
}:
let
  # Cheap stand-ins so eval does not pull Stalwart binary FOD or real arti.
  # arti = stock client-shaped fake (no surmountOnionServiceCapable).
  # artiOnionService = Surmount service-capable fake (passthru claim only).
  fakeOverlay = _final: prev: {
    stalwart-mail = (prev.writeShellScriptBin "stalwart" "exit 0") // {
      pname = "stalwart-mail-fake";
      spam-filter = null;
      webui = null;
    };
    stalwart-cli = prev.writeShellScriptBin "stalwart-cli" "exit 0";
    surmount-management-ui = prev.writeShellScriptBin "surmount-management-ui" "exit 0";
    arti = prev.writeShellScriptBin "arti" "exit 0" // {
      pname = "arti";
    };
    artiOnionService = prev.writeShellScriptBin "arti" "exit 0" // {
      pname = "arti-onion-service";
      passthru = {
        surmountOnionServiceCapable = true;
        onionServiceFeatures = [ "onion-service-service" ];
      };
      # passthru attrs are also visible at top-level on real derivations.
      surmountOnionServiceCapable = true;
    };
  };

  evalSurmount =
    extra:
    lib.nixosSystem {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        sops-nix.nixosModules.sops
        ../modules
        (
          { ... }:
          {
            nixpkgs.overlays = [ fakeOverlay ];
            nixpkgs.config.allowUnfree = false;
            system.stateVersion = "25.05";
            networking.hostName = "surmount-eval";
            fileSystems."/" = {
              device = "nodev";
              fsType = "ext4";
            };
            boot.loader.grub.enable = false;
            # mail.nix enables stalwart when surmount.enable; fake package is enough for eval.
            surmount = lib.mkMerge [
              {
                enable = true;
                # web.enable product default is false (Axum-first public edge).
                # Do not mkDefault-force it here; t1 asserts the real default.
                managementUi.enable = lib.mkDefault false;
                hardening.enable = lib.mkDefault false;
                backups.enable = lib.mkDefault false;
                artiHiddenService.enable = lib.mkDefault false;
                artiHiddenService.startDaemon = lib.mkDefault false;
                artiHiddenService.acceptIncompleteOnionConfig = lib.mkDefault false;
              }
              extra
            ];
          }
        )
      ];
    };

  failedAssertions = eval: builtins.filter (a: !a.assertion) eval.config.assertions;

  # --- contracts -------------------------------------------------------------

  t1-defaults =
    let
      e = evalSurmount { };
    in
    assert e.config.surmount.secrets.requireDeployMaterial == false;
    assert e.config.surmount.artiHiddenService.enable == false;
    assert e.config.surmount.artiHiddenService.startDaemon == false;
    assert e.config.surmount.artiHiddenService.acceptIncompleteOnionConfig == false;
    assert e.config.surmount.artiHiddenService.packageIsOnionServiceCapable == false;
    assert e.config.surmount.artiHiddenService.publishStalwartAdmin == false;
    assert e.config.surmount.artiHiddenService.publishStalwartJmap == false;
    assert e.config.surmount.artiHiddenService.backendAddress == null;
    assert e.config.surmount.managementUi.listenMode == "http";
    assert e.config.surmount.managementUi.allowCleartextHttpsEscape == false;
    # Product default: nginx transitional edge off (Axum-first public path).
    assert e.config.surmount.web.enable == false;
    assert e.config.services.nginx.enable == false;
    assert failedAssertions e == [ ];
    "t1-defaults-ok";

  t2-require-empty-paths-asserts =
    let
      e = evalSurmount {
        secrets.requireDeployMaterial = true;
        secrets.requiredHostPaths = [ ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "requiredHostPaths is empty" a.message) failed;
    "t2-require-empty-paths-asserts-ok";

  t3-bad-path-charset-asserts =
    let
      e = evalSurmount {
        secrets.requireDeployMaterial = true;
        secrets.requiredHostPaths = [
          {
            path = "/tmp/evil;rm -rf /";
            kind = "file";
          }
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "strict" a.message || lib.hasInfix "metacharacters" a.message
    ) failed;
    "t3-bad-path-charset-asserts-ok";

  t4-require-paths-wires-check =
    let
      e = evalSurmount {
        secrets.requireDeployMaterial = true;
        secrets.requiredHostPaths = [
          {
            path = "/run/surmount-secrets/tls/cert.pem";
            kind = "file";
          }
          {
            path = "/run/surmount-secrets/tls/key.pem";
            kind = "file";
          }
          {
            path = "/run/surmount-secrets/arti/onion-service";
            kind = "directory";
          }
        ];
      };
    in
    assert failedAssertions e == [ ];
    assert e.config.systemd.services ? surmount-deploy-secrets-check;
    assert e.config.system.activationScripts ? surmountDeploySecrets;
    # Script path is store path; unit ExecStart must be set.
    assert e.config.systemd.services.surmount-deploy-secrets-check.serviceConfig.ExecStart != null;
    "t4-require-paths-wires-check-ok";

  t5-arti-enable-no-daemon =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = false;
      };
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
    in
    assert failedAssertions e == [ ];
    assert
      !(e.config.systemd.services ? surmount-arti-hidden-service)
      || e.config.systemd.services.surmount-arti-hidden-service.enable == false
      || e.config.systemd.services.surmount-arti-hidden-service.wantedBy == [ ];
    # Status oneshot is present when not starting daemon.
    assert e.config.systemd.services ? surmount-arti-scaffold-status;
    assert e.config.environment.etc ? "surmount/arti.toml";
    # Real management-publish stanzas (not comment-only scaffold).
    assert lib.hasInfix "[onion_services." toml;
    assert lib.hasInfix "proxy_ports" toml;
    assert lib.hasInfix "state_dir" toml;
    assert lib.hasInfix "cache_dir" toml;
    assert !(lib.hasInfix "BEGIN PRIVATE" toml);
    assert !(lib.hasInfix "BEGIN PRIVATE" backend);
    assert !(lib.hasInfix "PRIVATE KEY" toml);
    assert lib.hasInfix "startDaemon=false" backend;
    assert lib.hasInfix "acceptIncompleteOnionConfig=false" backend;
    assert lib.hasInfix "configKind=management-publish" backend;
    # Lean defaults.
    assert e.config.surmount.artiHiddenService.publishStalwartAdmin == false;
    assert e.config.surmount.artiHiddenService.publishStalwartJmap == false;
    assert e.config.surmount.artiHiddenService.packageIsOnionServiceCapable == false;
    assert lib.hasInfix "publishStalwartAdmin=false" backend;
    assert lib.hasInfix "publishStalwartJmap=false" backend;
    # Derived backend tracks managementUi defaults when backendAddress is null.
    assert lib.hasInfix "127.0.0.1:8090" toml;
    "t5-arti-enable-no-daemon-ok";

  # Complete management-publish config: startDaemon does NOT need acceptIncomplete.
  t6-arti-start-daemon-complete-no-accept =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = true;
        artiHiddenService.acceptIncompleteOnionConfig = false;
        artiHiddenService.packageIsOnionServiceCapable = true;
      };
      failed = failedAssertions e;
      svc = e.config.systemd.services.surmount-arti-hidden-service;
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      hsDir = e.config.surmount.artiHiddenService.onionServiceStateDir;
    in
    assert failed == [ ];
    assert e.config.systemd.services ? surmount-arti-hidden-service;
    assert builtins.elem "multi-user.target" svc.wantedBy;
    assert svc.unitConfig.ConditionPathIsDirectory == hsDir;
    # Lock restart-loop guards (past footgun).
    assert svc.unitConfig.StartLimitBurst == 5;
    assert svc.unitConfig.StartLimitIntervalSec == 300;
    assert svc.serviceConfig.Restart == "on-failure";
    assert svc.serviceConfig.RestartSec != null;
    assert lib.hasInfix "[onion_services." toml;
    assert lib.hasInfix "proxy_ports" toml;
    assert lib.hasInfix "127.0.0.1:8090" toml;
    assert !(lib.hasInfix "BEGIN PRIVATE" toml);
    assert !(lib.hasInfix "PRIVATE KEY" toml);
    "t6-arti-start-daemon-complete-no-accept-ok";

  t6b-arti-start-daemon-ok =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = true;
        artiHiddenService.packageIsOnionServiceCapable = true;
        # package null prefers pkgs.artiOnionService from fake overlay
      };
      svc = e.config.systemd.services.surmount-arti-hidden-service;
      hsDir = e.config.surmount.artiHiddenService.onionServiceStateDir;
    in
    assert failedAssertions e == [ ];
    assert e.config.systemd.services ? surmount-arti-hidden-service;
    assert builtins.elem "multi-user.target" svc.wantedBy;
    # ConditionPathIsDirectory must gate missing HS identity dir.
    assert svc.unitConfig.ConditionPathIsDirectory == hsDir;
    assert svc.unitConfig.StartLimitBurst == 5;
    assert svc.unitConfig.StartLimitIntervalSec == 300;
    assert svc.serviceConfig.Restart == "on-failure";
    assert
      lib.hasInfix "arti proxy" svc.serviceConfig.ExecStart
      || lib.hasInfix "/bin/arti" svc.serviceConfig.ExecStart;
    "t6b-arti-start-daemon-ok";

  t6c-arti-start-daemon-needs-package =
    let
      # Overlay without arti so package null fails closed when startDaemon.
      noArtiOverlay = _final: prev: {
        stalwart-mail = (prev.writeShellScriptBin "stalwart" "exit 0") // {
          pname = "stalwart-mail-fake";
          spam-filter = null;
          webui = null;
        };
        stalwart-cli = prev.writeShellScriptBin "stalwart-cli" "exit 0";
        surmount-management-ui = prev.writeShellScriptBin "surmount-management-ui" "exit 0";
        # Hide arti if the channel provides it.
        arti = null;
      };
      e = lib.nixosSystem {
        system = pkgs.stdenv.hostPlatform.system;
        modules = [
          sops-nix.nixosModules.sops
          ../modules
          (
            { ... }:
            {
              nixpkgs.overlays = [ noArtiOverlay ];
              nixpkgs.config.allowUnfree = false;
              system.stateVersion = "25.05";
              networking.hostName = "surmount-eval-no-arti";
              fileSystems."/" = {
                device = "nodev";
                fsType = "ext4";
              };
              boot.loader.grub.enable = false;
              surmount = {
                enable = true;
                web.enable = false;
                managementUi.enable = false;
                hardening.enable = false;
                backups.enable = false;
                artiHiddenService.enable = true;
                artiHiddenService.startDaemon = true;
                artiHiddenService.package = null;
                artiHiddenService.packageIsOnionServiceCapable = true;
              };
            }
          )
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "no Arti package" a.message) failed;
    "t6c-arti-start-daemon-needs-package-ok";

  t6c2-arti-start-daemon-needs-service-capable =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = true;
        artiHiddenService.packageIsOnionServiceCapable = false;
        # Explicit stock client package so Surmount artiOnionService auto-claim
        # does not satisfy the gate (fail-closed residual for non-capable bins).
        artiHiddenService.package = pkgs.arti;
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "packageIsOnionServiceCapable" a.message
      || lib.hasInfix "onion-service-service" a.message
    ) failed;
    "t6c2-arti-start-daemon-needs-service-capable-ok";

  t6d-arti-uds-backend-in-toml =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.backendUnixSocket = "/run/surmount/management-ui.sock";
      };
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "unix:/run/surmount/management-ui.sock" toml;
    assert lib.hasInfix "unix:/run/surmount/management-ui.sock" backend;
    assert !(lib.hasInfix "BEGIN PRIVATE" toml);
    "t6d-arti-uds-backend-in-toml-ok";

  t6e-arti-bad-nickname-asserts =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.nickname = "bad nickname!";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "nickname" a.message) failed;
    "t6e-arti-bad-nickname-asserts-ok";

  # Reserved flags true must not add admin/JMAP onion stanzas (management only).
  t6f-arti-publish-flags-reserved-no-stanzas =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.publishStalwartAdmin = true;
        artiHiddenService.publishStalwartJmap = true;
      };
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
      # Count onion_services table headers (management nickname only).
      nick = e.config.surmount.artiHiddenService.nickname;
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "[onion_services.\"${nick}\"]" toml;
    assert lib.hasInfix "127.0.0.1:8090" toml;
    assert lib.hasInfix "publishStalwartAdmin=true" backend;
    assert lib.hasInfix "publishStalwartJmap=true" backend;
    # No second service table or invented admin backend ports.
    assert !(lib.hasInfix "onion_services.\"stalwart" toml);
    assert !(lib.hasInfix "onion_services.\"admin" toml);
    assert !(lib.hasInfix "8080" toml); # do not invent Stalwart admin port
    # Single proxy_ports block: only management 80 + destroy catch-all.
    assert lib.hasInfix "[\"80\"" toml || lib.hasInfix "[\"80\"," toml || lib.hasInfix "\"80\"" toml;
    "t6f-arti-publish-flags-reserved-no-stanzas-ok";

  # backendAddress null derives from managementUi listenAddress:port.
  t6g-arti-backend-tracks-ui-port =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        managementUi.port = 9099;
        managementUi.listenAddress = "127.0.0.1";
      };
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "127.0.0.1:9099" toml;
    assert lib.hasInfix "backend=127.0.0.1:9099" backend;
    assert !(lib.hasInfix "127.0.0.1:8090" toml);
    "t6g-arti-backend-tracks-ui-port-ok";

  t6h-arti-bad-backend-address-asserts =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.backendAddress = "not a host\nport\"evil\"";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "backendAddress" a.message) failed;
    "t6h-arti-bad-backend-address-asserts-ok";

  # acceptIncomplete=true does not weaken package / service-capable gates.
  t6i-arti-accept-incomplete-no-weaken =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = true;
        artiHiddenService.acceptIncompleteOnionConfig = true;
        artiHiddenService.packageIsOnionServiceCapable = false;
        # Force stock client package so Surmount artiOnionService auto-claim
        # cannot satisfy the gate (acceptIncomplete must not weaken it).
        artiHiddenService.package = pkgs.arti;
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "packageIsOnionServiceCapable" a.message
      || lib.hasInfix "onion-service-service" a.message
    ) failed;
    "t6i-arti-accept-incomplete-no-weaken-ok";

  # Happy path: null package + Surmount artiOnionService on pkgs auto-wires
  # capable (no manual packageIsOnionServiceCapable). Hermetic fake only.
  t6k-arti-surmount-package-auto-capable =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = true;
        # package null -> module prefers pkgs.artiOnionService
        # packageIsOnionServiceCapable default false; passthru claim is enough
      };
      failed = failedAssertions e;
      svc = e.config.systemd.services.surmount-arti-hidden-service;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
    in
    assert failed == [ ];
    assert e.config.systemd.services ? surmount-arti-hidden-service;
    assert builtins.elem "multi-user.target" svc.wantedBy;
    assert svc.unitConfig.ConditionPathIsDirectory != null;
    assert svc.unitConfig.StartLimitBurst == 5;
    assert
      lib.hasInfix "arti proxy" svc.serviceConfig.ExecStart
      || lib.hasInfix "/bin/arti" svc.serviceConfig.ExecStart;
    # Effective gate true via passthru; raw option stays false; Surmount pkg.
    assert lib.hasInfix "onionServiceCapable=true" backend;
    assert lib.hasInfix "packageClaimsOnionServiceCapable=true" backend;
    assert lib.hasInfix "packageIsOnionServiceCapable=false" backend;
    assert lib.hasInfix "package=arti-onion-service" backend;
    assert e.config.surmount.artiHiddenService.packageIsOnionServiceCapable == false;
    assert lib.hasInfix "[onion_services." toml;
    assert !(lib.hasInfix "BEGIN PRIVATE" toml);
    assert !(lib.hasInfix "PRIVATE KEY" toml);
    assert !(lib.hasInfix "hs_ed25519_secret_key" toml);
    "t6k-arti-surmount-package-auto-capable-ok";

  # Stock-only overlay: no artiOnionService, startDaemon without claim fails.
  t6k2-arti-stock-only-still-fail-closed =
    let
      stockOnlyOverlay = _final: prev: {
        stalwart-mail = (prev.writeShellScriptBin "stalwart" "exit 0") // {
          pname = "stalwart-mail-fake";
          spam-filter = null;
          webui = null;
        };
        stalwart-cli = prev.writeShellScriptBin "stalwart-cli" "exit 0";
        surmount-management-ui = prev.writeShellScriptBin "surmount-management-ui" "exit 0";
        arti = prev.writeShellScriptBin "arti" "exit 0" // {
          pname = "arti";
        };
        # Deliberately no artiOnionService attribute.
      };
      e = lib.nixosSystem {
        system = pkgs.stdenv.hostPlatform.system;
        modules = [
          sops-nix.nixosModules.sops
          ../modules
          (
            { ... }:
            {
              nixpkgs.overlays = [ stockOnlyOverlay ];
              nixpkgs.config.allowUnfree = false;
              system.stateVersion = "25.05";
              networking.hostName = "surmount-eval-stock-arti";
              fileSystems."/" = {
                device = "nodev";
                fsType = "ext4";
              };
              boot.loader.grub.enable = false;
              surmount = {
                enable = true;
                web.enable = false;
                managementUi.enable = false;
                hardening.enable = false;
                backups.enable = false;
                artiHiddenService.enable = true;
                artiHiddenService.startDaemon = true;
                artiHiddenService.packageIsOnionServiceCapable = false;
              };
            }
          )
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "packageIsOnionServiceCapable" a.message
      || lib.hasInfix "onion-service-service" a.message
    ) failed;
    "t6k2-arti-stock-only-still-fail-closed-ok";

  # Explicit stock pkgs.arti package even when artiOnionService exists: still
  # fail closed unless operator sets packageIsOnionServiceCapable (footgun gate).
  t6k3-explicit-stock-package-needs-claim =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = true;
        artiHiddenService.package = pkgs.arti;
        artiHiddenService.packageIsOnionServiceCapable = false;
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "packageIsOnionServiceCapable" a.message
      || lib.hasInfix "onion-service-service" a.message
    ) failed;
    "t6k3-explicit-stock-package-needs-claim-ok";

  # HTTPS UI + arti, no explicit backend: auto local cleartext API + onion target.
  # No mismatch warning (backend is not the primary https TCP).
  t6j-arti-https-auto-local-cleartext =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
      isMismatch = w: lib.hasInfix "cleartext" w && lib.hasInfix "artiHiddenService" w;
    in
    assert failedAssertions e == [ ];
    # Auto cleartext for public https primary -> 127.0.0.1:8090
    assert builtins.any (x: x == "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=127.0.0.1:8090") env;
    assert lib.hasInfix "127.0.0.1:8090" toml;
    assert lib.hasInfix "backend=127.0.0.1:8090" backend;
    # Must not point Arti at the TLS primary.
    assert !(lib.hasInfix "0.0.0.0:443" toml);
    assert !(builtins.any isMismatch e.config.warnings);
    "t6j-arti-https-auto-local-cleartext-ok";

  # Explicit backendAddress equal to primary https TCP still warns (misconfig).
  t6j-misconfig-explicit-https-tcp-warns =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        # Force onion at the primary https listen (operator footgun).
        artiHiddenService.backendAddress = "0.0.0.0:443";
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      warns = e.config.warnings;
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
    in
    assert failedAssertions e == [ ];
    assert warns != [ ];
    assert builtins.any (w: lib.hasInfix "cleartext" w && lib.hasInfix "listenMode" w) warns;
    # No auto local cleartext when backend is explicit (operator owns path).
    assert !(builtins.any (x: lib.hasPrefix "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=" x) env);
    "t6j-misconfig-explicit-https-tcp-warns-ok";

  # Escape-on, off-UI TCP backend, UDS backend, or auto cleartext: no mismatch warn.
  t6j2-arti-https-mismatch-suppressed =
    let
      eEscape = evalSurmount {
        artiHiddenService.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = true;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      eOffUi = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.backendAddress = "127.0.0.1:19999";
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      eUds = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.backendUnixSocket = "/run/surmount/management-ui.sock";
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      eAuto = evalSurmount {
        artiHiddenService.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 8090;
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      isMismatch = w: lib.hasInfix "cleartext" w && lib.hasInfix "artiHiddenService" w;
      envAuto = eAuto.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      tomlAuto = builtins.readFile eAuto.config.environment.etc."surmount/arti.toml".source;
    in
    assert failedAssertions eEscape == [ ];
    assert failedAssertions eOffUi == [ ];
    assert failedAssertions eUds == [ ];
    assert failedAssertions eAuto == [ ];
    assert !(builtins.any isMismatch eEscape.config.warnings);
    assert !(builtins.any isMismatch eOffUi.config.warnings);
    assert !(builtins.any isMismatch eUds.config.warnings);
    assert !(builtins.any isMismatch eAuto.config.warnings);
    # Loopback https on 8090 auto-derives sibling 8091.
    assert builtins.any (x: x == "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=127.0.0.1:8091") envAuto;
    assert lib.hasInfix "127.0.0.1:8091" tomlAuto;
    # UDS backend marker present; no auto cleartext bind when UDS explicit.
    assert lib.hasInfix "unix:/run/surmount/management-ui.sock" (
      builtins.readFile eUds.config.environment.etc."surmount/arti.toml".source
    );
    assert
      !(builtins.any (
        x: lib.hasPrefix "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=" x
      ) eUds.config.systemd.services.surmount-management-ui.serviceConfig.Environment);
    "t6j2-arti-https-mismatch-suppressed-ok";

  # Explicit localCleartextListen override is honored for UI env + Arti target.
  t6j3-explicit-local-cleartext-override =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.localCleartextListen = "127.0.0.1:9191";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: x == "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=127.0.0.1:9191") env;
    assert lib.hasInfix "backend=127.0.0.1:9191" backend;
    "t6j3-explicit-local-cleartext-override-ok";

  # Non-loopback local cleartext fails closed (no public cleartext API).
  t6j4-local-cleartext-must-be-loopback =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.localCleartextListen = "0.0.0.0:8090";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "loopback" a.message || lib.hasInfix "local cleartext" a.message
    ) failed;
    "t6j4-local-cleartext-must-be-loopback-ok";

  # Public primary on port 8090 must not auto-emit 127.0.0.1:8090 (bind collision).
  t6j5-auto-cleartext-avoids-primary-port =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 8090;
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      backend = e.config.environment.etc."surmount/arti-backend-target".text;
    in
    assert failedAssertions e == [ ];
    # Sibling 8091 (8090 taken by primary port).
    assert builtins.any (x: x == "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=127.0.0.1:8091") env;
    assert lib.hasInfix "127.0.0.1:8091" toml;
    assert lib.hasInfix "backend=127.0.0.1:8091" backend;
    assert !(builtins.any (x: x == "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=127.0.0.1:8090") env);
    "t6j5-auto-cleartext-avoids-primary-port-ok";

  # localhost: hostname is not a SocketAddr for the binary; fail closed at eval.
  t6j6-localhost-hostname-fail-closed =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.localCleartextListen = "localhost:8090";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "loopback" a.message || lib.hasInfix "local cleartext" a.message
    ) failed;
    "t6j6-localhost-hostname-fail-closed-ok";

  # Explicit same-port collision (wildcard primary vs loopback cleartext) fail-closed.
  t6j7-explicit-same-port-collision-fail-closed =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 8090;
        managementUi.localCleartextListen = "127.0.0.1:8090";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "collid" a.message || lib.hasInfix "port" a.message) failed;
    "t6j7-explicit-same-port-collision-fail-closed-ok";

  t7-https-tls-paths-must-be-strict =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = true;
        managementUi.tlsCertPath = "relative/not/ok.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    "t7-https-tls-paths-strict-ok";

  # Bare https with PEMs is OK now that in-process rustls is wired.
  t8-https-bare-with-pems-ok =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      uc = e.config.systemd.services.surmount-management-ui.unitConfig;
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: lib.hasPrefix "SURMOUNT_LISTEN_MODE=https" x) env;
    assert builtins.any (x: lib.hasPrefix "SURMOUNT_TLS_CERT=" x) env;
    assert builtins.any (x: lib.hasPrefix "SURMOUNT_TLS_KEY=" x) env;
    # Escape env must not be injected on the happy path.
    assert !(builtins.any (x: x == "SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1") env);
    # Restart caps always present (fail-closed binary must not thrash forever).
    assert uc.StartLimitBurst == 5;
    assert uc.StartLimitIntervalSec == 300;
    "t8-https-bare-with-pems-ok";

  # https without PEM path strings must fail eval (no silent empty-path https).
  t8b-https-empty-pem-paths-asserts =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.tlsCertPath = "";
        managementUi.tlsKeyPath = "";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "tlsCertPath" a.message || lib.hasInfix "tlsKeyPath" a.message
    ) failed;
    "t8b-https-empty-pem-paths-asserts-ok";

  t9-https-with-escape-ok =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = true;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      uc = e.config.systemd.services.surmount-management-ui.unitConfig;
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: lib.hasPrefix "SURMOUNT_LISTEN_MODE=https" x) env;
    assert builtins.any (x: x == "SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1") env;
    # Escape path still gets restart caps; ConditionPathExists only on real TLS.
    assert uc.StartLimitBurst == 5;
    assert
      !(builtins.hasAttr "ConditionPathExists" uc)
      || uc.ConditionPathExists == null
      || uc.ConditionPathExists == [ ];
    "t9-https-with-escape-ok";

  t9b-escape-without-https-asserts =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "http";
        managementUi.allowCleartextHttpsEscape = true;
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "allowCleartextHttpsEscape" a.message) failed;
    "t9b-escape-without-https-asserts-ok";

  # Product path: web.enable=false (default or explicit) => nginx not product edge;
  # management-ui https with host PEM paths evaluates (no secrets in tree).
  t10-web-off-https-ui-no-nginx =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
      uc = e.config.systemd.services.surmount-management-ui.unitConfig;
      cond =
        if builtins.isList uc.ConditionPathExists then
          uc.ConditionPathExists
        else
          [ uc.ConditionPathExists ];
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.web.enable == false;
    assert e.config.services.nginx.enable == false;
    assert builtins.any (x: x == "SURMOUNT_LISTEN=0.0.0.0:443") env;
    assert builtins.any (x: lib.hasPrefix "SURMOUNT_LISTEN_MODE=https" x) env;
    assert builtins.any (x: x == "SURMOUNT_TLS_CERT=/run/surmount-secrets/tls/cert.pem") env;
    assert builtins.any (x: x == "SURMOUNT_TLS_KEY=/run/surmount-secrets/tls/key.pem") env;
    assert !(builtins.any (x: x == "SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1") env);
    # MDWE remains on https path (operator host smoke still residual).
    assert sc.MemoryDenyWriteExecute == true;
    assert builtins.elem "/run/surmount-secrets/tls/cert.pem" sc.ReadOnlyPaths;
    assert builtins.elem "/run/surmount-secrets/tls/key.pem" sc.ReadOnlyPaths;
    # Restart caps + missing-PEM gate (no restart thrash when files absent).
    assert uc.StartLimitBurst == 5;
    assert uc.StartLimitIntervalSec == 300;
    assert builtins.elem "/run/surmount-secrets/tls/cert.pem" cond;
    assert builtins.elem "/run/surmount-secrets/tls/key.pem" cond;
    "t10-web-off-https-ui-no-nginx-ok";

  # Transitional escape: surmount.web.enable = true still wires nginx + ACME.
  # Documented shape: UI loopback http behind nginx (not public https).
  t11-web-dual-run-escape-nginx-on =
    let
      e = evalSurmount {
        web.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "http";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 8090;
      };
      vhosts = e.config.services.nginx.virtualHosts;
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.web.enable == true;
    assert e.config.services.nginx.enable == true;
    assert vhosts ? "services.surmount.systems";
    assert vhosts."services.surmount.systems".enableACME == true;
    assert vhosts."services.surmount.systems".forceSSL == true;
    assert builtins.any (x: x == "SURMOUNT_LISTEN=127.0.0.1:8090") env;
    assert builtins.any (x: x == "SURMOUNT_LISTEN_MODE=http") env;
    "t11-web-dual-run-escape-nginx-on-ok";

  # Dual-run + public Axum HTTPS must fail closed (two owners of :443).
  t12-dual-run-public-https-asserts =
    let
      ePort = evalSurmount {
        web.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 443;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      ePublic = evalSurmount {
        web.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 8443;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      # Loopback https on non-443 is allowed (nginx still public edge).
      eOk = evalSurmount {
        web.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 8443;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      isConflict = a: lib.hasInfix "web.enable" a.message && lib.hasInfix "https" a.message;
    in
    assert failedAssertions ePort != [ ];
    assert builtins.any isConflict (failedAssertions ePort);
    assert failedAssertions ePublic != [ ];
    assert builtins.any isConflict (failedAssertions ePublic);
    assert failedAssertions eOk == [ ];
    "t12-dual-run-public-https-asserts-ok";

  # Product :80 redirect bind: web off + https + redirectHttpToHttps wires env
  # and CAP_NET_BIND_SERVICE; no nginx.
  t13-http-redirect-bind-env-and-caps =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.allowCleartextHttpsEscape = false;
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "0.0.0.0:80";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
      uc = e.config.systemd.services.surmount-management-ui.unitConfig;
    in
    assert failedAssertions e == [ ];
    assert e.config.services.nginx.enable == false;
    assert builtins.any (x: x == "SURMOUNT_REDIRECT_HTTP_TO_HTTPS=true") env;
    assert builtins.any (x: x == "SURMOUNT_HTTP_REDIRECT_LISTEN=0.0.0.0:80") env;
    assert builtins.any (x: x == "SURMOUNT_LISTEN=0.0.0.0:443") env;
    assert builtins.any (x: lib.hasPrefix "SURMOUNT_LISTEN_MODE=https" x) env;
    # Privileged :80/:443: non-root needs CAP_NET_BIND_SERVICE.
    assert sc.AmbientCapabilities == [ "CAP_NET_BIND_SERVICE" ];
    assert sc.CapabilityBoundingSet == [ "CAP_NET_BIND_SERVICE" ];
    # Restart caps still present (bind fail-closed must not thrash forever).
    assert uc.StartLimitBurst == 5;
    assert uc.StartLimitIntervalSec == 300;
    assert sc.MemoryDenyWriteExecute == true;
    "t13-http-redirect-bind-env-and-caps-ok";

  # Flag off: no SURMOUNT_HTTP_REDIRECT_LISTEN even if option default is :80.
  t13b-redirect-flag-off-no-listen-env =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.redirectHttpToHttps = false;
        # default httpRedirectListen remains 0.0.0.0:80 but must not be emitted
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: x == "SURMOUNT_REDIRECT_HTTP_TO_HTTPS=false") env;
    assert !(builtins.any (x: lib.hasPrefix "SURMOUNT_HTTP_REDIRECT_LISTEN=" x) env);
    "t13b-redirect-flag-off-no-listen-env-ok";

  # Empty httpRedirectListen with flag on: flag env true, no listen env (skip bind).
  t13c-redirect-empty-listen-skips-env =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: x == "SURMOUNT_REDIRECT_HTTP_TO_HTTPS=true") env;
    assert !(builtins.any (x: lib.hasPrefix "SURMOUNT_HTTP_REDIRECT_LISTEN=" x) env);
    # Primary still loopback 8090 default: no CAP_NET_BIND from redirect.
    assert
      !(builtins.hasAttr "AmbientCapabilities" sc)
      || sc.AmbientCapabilities == null
      || sc.AmbientCapabilities == [ ];
    "t13c-redirect-empty-listen-skips-env-ok";

  # Dual-run nginx + product redirectHttpToHttps must fail closed (:80 mutex).
  # listenMode must be https for the redirect-requires-https gate; dual-run
  # public https is a separate mutex, so use loopback https non-443.
  t14-dual-run-redirect-mutex-asserts =
    let
      e = evalSurmount {
        web.enable = true;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 8443;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "0.0.0.0:80";
      };
      failed = failedAssertions e;
      isRedirectMutex =
        a:
        lib.hasInfix "redirectHttpToHttps" a.message
        || (lib.hasInfix "web.enable" a.message && lib.hasInfix ":80" a.message);
    in
    assert failed != [ ];
    assert builtins.any isRedirectMutex failed;
    "t14-dual-run-redirect-mutex-asserts-ok";

  # redirectHttpToHttps requires listenMode=https (no half-wired http primary).
  t15-redirect-requires-https-asserts =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "http";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 8090;
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "0.0.0.0:80";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "redirectHttpToHttps" a.message && lib.hasInfix "https" a.message
    ) failed;
    "t15-redirect-requires-https-asserts-ok";

  # Bad listen shape fails eval (not silent binary skip).
  t16-redirect-listen-shape-asserts =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "0.0.0.0.80";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "httpRedirectListen" a.message || lib.hasInfix "host:port" a.message
    ) failed;
    "t16-redirect-listen-shape-asserts-ok";

  # High-port redirect + non-privileged primary: no CAP_NET_BIND_SERVICE.
  t17-high-port-redirect-no-cap =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 8443;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.redirectHttpToHttps = true;
        managementUi.httpRedirectListen = "127.0.0.1:18080";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: x == "SURMOUNT_HTTP_REDIRECT_LISTEN=127.0.0.1:18080") env;
    assert
      !(builtins.hasAttr "AmbientCapabilities" sc)
      || sc.AmbientCapabilities == null
      || sc.AmbientCapabilities == [ ];
    "t17-high-port-redirect-no-cap-ok";

  # accessControl defaults lean private (off).
  t18-access-control-defaults-off =
    let
      e = evalSurmount { };
    in
    assert e.config.surmount.accessControl.enable == false;
    assert e.config.surmount.accessControl.enforcement == "off";
    assert e.config.surmount.accessControl.backend == "memory";
    assert e.config.surmount.accessControl.nftExec == false;
    assert e.config.surmount.accessControl.nftHelper == false;
    assert e.config.surmount.accessControl.nftSets == true; # only applies when enable
    assert failedAssertions e == [ ];
    # nft guard table not installed when accessControl disabled.
    assert !(e.config.networking.nftables.tables ? surmount_guard);
    "t18-access-control-defaults-off-ok";

  # enable + nftSets wires canonical set names (match Rust ban.rs).
  t19-access-control-nft-sets =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.nftSets = true;
        managementUi.enable = true;
        managementUi.listenMode = "http";
      };
      tables = e.config.networking.nftables.tables;
      content = tables.surmount_guard.content;
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = lib.concatStringsSep "\n" envList;
    in
    assert failedAssertions e == [ ];
    assert e.config.networking.nftables.enable == true;
    assert tables ? surmount_guard;
    assert tables.surmount_guard.family == "inet";
    assert lib.hasInfix "surmount-ban4" content;
    assert lib.hasInfix "surmount-ban6" content;
    assert lib.hasInfix "surmount-whitelist4" content;
    assert lib.hasInfix "surmount-whitelist6" content;
    # UI ban env wired; default enforcement still off (lean private).
    assert lib.hasInfix "SURMOUNT_BAN_ENFORCEMENT=off" envBlob;
    assert lib.hasInfix "SURMOUNT_BAN_BACKEND=memory" envBlob;
    assert lib.hasInfix "SURMOUNT_BAN_STATE_PATH=" envBlob;
    assert lib.hasInfix "ban-state.json" envBlob;
    # No in-process nft exec by default (no CAP_NET_ADMIN grant for bans).
    assert !(lib.hasInfix "SURMOUNT_BAN_NFT_EXEC" envBlob);
    "t19-access-control-nft-sets-ok";

  # nftExec without absolute nftBin asserts fail-closed.
  t20-access-control-nft-exec-needs-bin =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.backend = "nft";
        accessControl.nftExec = true;
        accessControl.nftBin = "";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "nftBin" a.message || lib.hasInfix "nftExec" a.message) failed;
    "t20-access-control-nft-exec-needs-bin-ok";

  # nftExec true: warning + still no CAP_NET_ADMIN on UI unit (footgun gate).
  t20b-access-control-nft-exec-no-cap-net-admin =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.backend = "nft";
        accessControl.nftExec = true;
        accessControl.nftBin = "/run/current-system/sw/bin/nft";
        managementUi.enable = true;
        managementUi.listenMode = "http";
      };
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
      warns = e.config.warnings;
      hasCap =
        builtins.hasAttr "AmbientCapabilities" sc
        && sc.AmbientCapabilities != null
        && sc.AmbientCapabilities != [ ]
        && (
          if builtins.isList sc.AmbientCapabilities then
            builtins.any (c: lib.hasInfix "CAP_NET_ADMIN" (toString c)) sc.AmbientCapabilities
          else
            lib.hasInfix "CAP_NET_ADMIN" (toString sc.AmbientCapabilities)
        );
    in
    assert failedAssertions e == [ ];
    assert warns != [ ];
    assert builtins.any (w: lib.hasInfix "CAP_NET_ADMIN" w || lib.hasInfix "nftExec" w) warns;
    assert !hasCap;
    # NoNewPrivileges remains true (product hardening).
    assert sc.NoNewPrivileges == true;
    "t20b-access-control-nft-exec-no-cap-net-admin-ok";

  # nftHelper + nftExec mutually exclusive.
  t20c-access-control-helper-xor-exec =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.backend = "nft";
        accessControl.nftExec = true;
        accessControl.nftHelper = true;
        accessControl.nftBin = "/run/current-system/sw/bin/nft";
        accessControl.nftHelperBin = "/run/wrappers/bin/surmount-nft-ban-helper";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "mutually exclusive" a.message || lib.hasInfix "nftHelper" a.message
    ) failed;
    "t20c-access-control-helper-xor-exec-ok";

  # nftHelper: UDS env + socket-activated oneshot caps; UI still no CAP_NET_ADMIN.
  # Elevation is helper unit AmbientCapabilities, NOT child setcap under NNP.
  t20d-access-control-nft-helper-least-privilege =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.nftHelper = true;
        accessControl.nftBin = "/run/current-system/sw/bin/nft";
        accessControl.nftHelperBin = "/nix/store/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-helper/bin/surmount-nft-ban-helper";
        accessControl.backend = "nft";
        accessControl.enforcement = "enforce";
        managementUi.enable = true;
        managementUi.listenMode = "http";
      };
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
      env = sc.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = lib.concatStringsSep "\n" envList;
      uiHasNetAdmin =
        builtins.hasAttr "AmbientCapabilities" sc
        && sc.AmbientCapabilities != null
        && sc.AmbientCapabilities != [ ]
        && (
          if builtins.isList sc.AmbientCapabilities then
            builtins.any (c: lib.hasInfix "CAP_NET_ADMIN" (toString c)) sc.AmbientCapabilities
          else
            lib.hasInfix "CAP_NET_ADMIN" (toString sc.AmbientCapabilities)
        );
      sock = e.config.systemd.sockets.surmount-nft-ban-helper;
      helperSvc = e.config.systemd.services."surmount-nft-ban-helper@";
      helperCaps = helperSvc.serviceConfig.AmbientCapabilities;
      helperCapBlob =
        if builtins.isList helperCaps then
          lib.concatStringsSep " " (map toString helperCaps)
        else
          toString helperCaps;
      bounding = helperSvc.serviceConfig.CapabilityBoundingSet;
      boundingBlob =
        if builtins.isList bounding then
          lib.concatStringsSep " " (map toString bounding)
        else
          toString bounding;
      uiUnit = e.config.systemd.services.surmount-management-ui;
      wantsAfter = (uiUnit.wants or [ ]) ++ (uiUnit.after or [ ]);
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "SURMOUNT_BAN_NFT_HELPER_SOCK=/run/surmount/nft-ban-helper.sock" envBlob;
    assert !(lib.hasInfix "SURMOUNT_BAN_NFT_HELPER=" envBlob);
    assert lib.hasInfix "SURMOUNT_BAN_NFT_BIN=/run/current-system/sw/bin/nft" envBlob;
    assert !(lib.hasInfix "SURMOUNT_BAN_NFT_EXEC" envBlob);
    assert !uiHasNetAdmin;
    assert sc.NoNewPrivileges == true;
    # No security.wrappers elevation (dead under UI NNP).
    assert !(e.config.security.wrappers ? surmount-nft-ban-helper);
    assert sock.socketConfig.Accept == true;
    assert sock.socketConfig.ListenStream == "/run/surmount/nft-ban-helper.sock";
    assert lib.hasInfix "CAP_NET_ADMIN" helperCapBlob;
    assert lib.hasInfix "CAP_NET_RAW" helperCapBlob;
    assert lib.hasInfix "CAP_NET_ADMIN" boundingBlob;
    assert lib.hasInfix "CAP_NET_RAW" boundingBlob;
    assert helperSvc.serviceConfig.NoNewPrivileges == true;
    assert lib.hasInfix "apply-systemd-socket" (toString helperSvc.serviceConfig.ExecStart);
    assert builtins.any (x: x == "surmount-nft-ban-helper.socket") wantsAfter;
    "t20d-access-control-nft-helper-least-privilege-ok";

  # nftHelper without nftBin asserts fail-closed.
  t20e-access-control-nft-helper-needs-nft-bin =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.nftHelper = true;
        accessControl.backend = "nft";
        accessControl.nftBin = "";
        accessControl.nftHelperBin = "/run/wrappers/bin/surmount-nft-ban-helper";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "nftBin" a.message || lib.hasInfix "nftHelper" a.message
    ) failed;
    "t20e-access-control-nft-helper-needs-nft-bin-ok";

  # nftHelper + backend=memory is fail-closed (would install sock with no apply).
  t20f-access-control-nft-helper-needs-backend-nft =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.nftHelper = true;
        accessControl.backend = "memory";
        accessControl.nftBin = "/run/current-system/sw/bin/nft";
        accessControl.nftHelperBin = "/nix/store/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-helper/bin/surmount-nft-ban-helper";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "backend" a.message
      || lib.hasInfix "nftHelper" a.message
      || lib.hasInfix "memory" a.message
    ) failed;
    "t20f-access-control-nft-helper-needs-backend-nft-ok";

  # enforce + whitelist env when operator opts in.
  t21-access-control-enforce-whitelist-env =
    let
      e = evalSurmount {
        accessControl.enable = true;
        accessControl.enforcement = "enforce";
        accessControl.backend = "memory";
        accessControl.whitelist = [
          "203.0.113.10/32"
          "2001:db8::/64"
        ];
        accessControl.nftSets = false;
        managementUi.enable = true;
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = lib.concatStringsSep "\n" envList;
    in
    assert failedAssertions e == [ ];
    assert !(e.config.networking.nftables.tables ? surmount_guard);
    assert lib.hasInfix "SURMOUNT_BAN_ENFORCEMENT=enforce" envBlob;
    assert lib.hasInfix "SURMOUNT_BAN_WHITELIST=" envBlob;
    assert lib.hasInfix "203.0.113.10/32" envBlob;
    assert lib.hasInfix "2001:db8::/64" envBlob;
    "t21-access-control-enforce-whitelist-env-ok";

  results = [
    t1-defaults
    t2-require-empty-paths-asserts
    t3-bad-path-charset-asserts
    t4-require-paths-wires-check
    t5-arti-enable-no-daemon
    t6-arti-start-daemon-complete-no-accept
    t6b-arti-start-daemon-ok
    t6c-arti-start-daemon-needs-package
    t6c2-arti-start-daemon-needs-service-capable
    t6d-arti-uds-backend-in-toml
    t6e-arti-bad-nickname-asserts
    t6f-arti-publish-flags-reserved-no-stanzas
    t6g-arti-backend-tracks-ui-port
    t6h-arti-bad-backend-address-asserts
    t6i-arti-accept-incomplete-no-weaken
    t6k-arti-surmount-package-auto-capable
    t6k2-arti-stock-only-still-fail-closed
    t6k3-explicit-stock-package-needs-claim
    t6j-arti-https-auto-local-cleartext
    t6j-misconfig-explicit-https-tcp-warns
    t6j2-arti-https-mismatch-suppressed
    t6j3-explicit-local-cleartext-override
    t6j4-local-cleartext-must-be-loopback
    t6j5-auto-cleartext-avoids-primary-port
    t6j6-localhost-hostname-fail-closed
    t6j7-explicit-same-port-collision-fail-closed
    t7-https-tls-paths-must-be-strict
    t8-https-bare-with-pems-ok
    t8b-https-empty-pem-paths-asserts
    t9-https-with-escape-ok
    t9b-escape-without-https-asserts
    t10-web-off-https-ui-no-nginx
    t11-web-dual-run-escape-nginx-on
    t12-dual-run-public-https-asserts
    t13-http-redirect-bind-env-and-caps
    t13b-redirect-flag-off-no-listen-env
    t13c-redirect-empty-listen-skips-env
    t14-dual-run-redirect-mutex-asserts
    t15-redirect-requires-https-asserts
    t16-redirect-listen-shape-asserts
    t17-high-port-redirect-no-cap
    t18-access-control-defaults-off
    t19-access-control-nft-sets
    t20-access-control-nft-exec-needs-bin
    t20b-access-control-nft-exec-no-cap-net-admin
    t20c-access-control-helper-xor-exec
    t20d-access-control-nft-helper-least-privilege
    t20e-access-control-nft-helper-needs-nft-bin
    t20f-access-control-nft-helper-needs-backend-nft
    t21-access-control-enforce-whitelist-env
  ];
in
{
  inherit results;
  ok = results;
}
