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
  # Imported splora NixOS module (flake input). Needed so extraGroups and
  # unitsMemoryMax contracts can turn services.splora.enable without IFD.
  sploraNixosModule ? null,
}:
let
  # Cheap stand-ins so eval does not pull Stalwart binary FOD or real arti.
  # arti = stock client-shaped fake (no surmountOnionServiceCapable).
  # artiOnionService = Surmount service-capable fake (passthru claim only).
  # Cheap stand-in so vaultwarden enable does not pull real package FOD.
  fakeVaultwarden =
    let
      bin = pkgs.writeShellScriptBin "vaultwarden" "exit 0";
    in
    bin
    // {
      pname = "vaultwarden-fake";
      # Stock module: package.override { dbBackend = ... }.
      override = _: bin // { pname = "vaultwarden-fake"; };
      webvault = pkgs.writeTextDir "share/vaultwarden/vault/index.html" "ok";
    };

  fakeOverlay = _final: prev: {
    stalwart-mail = (prev.writeShellScriptBin "stalwart" "exit 0") // {
      pname = "stalwart-mail-fake";
      spam-filter = null;
      webui = null;
    };
    stalwart-cli = prev.writeShellScriptBin "stalwart-cli" "exit 0";
    # mail.nix puts pkgs.vandelay on systemPackages. t36 (and any
    # systemPackages walk) forces that attr; keep it a cheap fake.
    vandelay = prev.writeShellScriptBin "vandelay" "exit 0";
    surmount-mail-import = prev.writeShellScriptBin "surmount-mail-import-maildir" "exit 0";
    surmount-deploy-host = prev.writeShellScriptBin "surmount-deploy-host" "exit 0";
    surmount-management-ui = prev.writeShellScriptBin "surmount-management-ui" "exit 0";
    # Packaged DNS-01 hook **code** (fake; real package is nix/packages/...).
    acme-dns-hook-namecheap = prev.writeShellScriptBin "acme-dns-hook-namecheap" "exit 0";
    # Packaged apex/www site (fake; real package is nix/packages/surmount-public-site.nix).
    surmount-public-site =
      (prev.writeTextDir "index.html" ''
        <title>Surmount Systems</title>
        BIP 360 Grok OSS
      '')
      // {
        pname = "surmount-public-site";
      };
    vaultwarden = fakeVaultwarden;
    arti = prev.writeShellScriptBin "arti" "exit 0" // {
      pname = "arti";
    };
    surmount-niced-builder = prev.writeShellScriptBin "surmount-niced-builder" "exit 0";
    surmount-scram = prev.writeShellScriptBin "surmount-scram" "exit 0";
    artiOnionService = prev.writeShellScriptBin "arti" "exit 0" // {
      pname = "arti-onion-service";
      passthru = {
        surmountOnionServiceCapable = true;
        onionServiceFeatures = [ "onion-service-service" ];
      };
      # passthru attrs are also visible at top-level on real derivations.
      surmountOnionServiceCapable = true;
    };
    # services.splora.package default is pkgs.splora. Keep this a cheap fake
    # so extraGroups eval never instantiates crane / rocksdb.
    splora = fakeSplora;
    splora-liquid = fakeSplora;
  };

  fakeSplora =
    pkgs.runCommand "splora-fake" { } ''
      mkdir -p "$out/bin"
      for b in splora splora-queue splora-import popular-scripts; do
        printf '%s\n' "#!${pkgs.runtimeShell}" "exit 0" > "$out/bin/$b"
        chmod +x "$out/bin/$b"
      done
    ''
    // {
      pname = "splora";
      meta.mainProgram = "splora";
    };

  # extra is surmount attrs. Optional __extraModules: list of extra NixOS modules
  # (for host-level services.* mkForce, etc.) stripped before surmount merge.
  evalSurmount =
    extra:
    let
      extraModules = extra.__extraModules or [ ];
      surmountExtra = builtins.removeAttrs extra [ "__extraModules" ];
    in
    lib.nixosSystem {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        sops-nix.nixosModules.sops
        ../modules
      ]
      ++ lib.optional (sploraNixosModule != null) sploraNixosModule
      ++ [
        (
          { ... }:
          {
            nixpkgs.overlays = [ fakeOverlay ];
            nixpkgs.config.allowUnfree = false;
            system.stateVersion = "26.05";
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
              surmountExtra
            ];
          }
        )
      ]
      ++ extraModules;
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
    assert e.config.surmount.managementUi.consoleAccountsFile == "";
    assert e.config.surmount.managementUi.nwcStoreFile == "";
    assert e.config.surmount.managementUi.directory == "unavailable";
    # Product default: nginx transitional edge off (Axum-first public path).
    assert e.config.surmount.web.enable == false;
    assert e.config.services.nginx.enable == false;
    # Remote builder cap is opt-in (host-local). Scaffold off.
    assert e.config.surmount.remoteBuilder.enable == false;
    # Paper trail default on: persistent journal with a size cap (not a SKU).
    assert e.config.surmount.logging.enable == true;
    assert e.config.surmount.logging.systemMaxUse == "1G";
    assert e.config.surmount.logging.runtimeMaxUse == "256M";
    assert e.config.surmount.remoteBuilder.memoryMax == "4G";
    assert e.config.surmount.remoteBuilder.cpuQuota == "auto";
    assert e.config.surmount.remoteBuilder.maxJobs == 8;
    assert e.config.surmount.remoteBuilder.diskGuardPercent == 95;
    assert e.config.surmount.lake.enable == false;
    assert e.config.surmount.lake.jobs == 4;
    # Grok OSS is opt-in (host-local). Scaffold off; no grok user.
    assert e.config.surmount.grokOss.enable == false;
    assert e.config.surmount.grokOss.package == null;
    assert e.config.surmount.grokOss.user == "grok";
    assert e.config.surmount.grokOss.memoryMax == "4G";
    assert e.config.surmount.logging.rateLimitBurst == "50000";
    # Domain C Vaultwarden stays off on sample defaults (no production footgun).
    assert e.config.surmount.vaultwarden.enable == false;
    assert e.config.services.vaultwarden.enable or false == false;
    assert e.config.surmount.managementUi.vaultwardenUrl == "";
    # Splora Unix proxy default off. HTTP/3 option default on; UDP 443 only
    # when listenMode is https (plain HTTP default must not open it).
    assert e.config.surmount.sploraProxy.enable == false;
    assert e.config.surmount.sploraProxy.unitsMemoryMax == null;
    assert e.config.surmount.sploraIndexer.enable == false;
    assert e.config.surmount.sploraIndexer.dbBlockCacheMb == 24;
    assert e.config.surmount.sploraIndexer.jsonrpcImport == true;
    assert e.config.surmount.sploraIndexer.daemonDir == null;
    assert e.config.surmount.sploraIndexer.publicHealth == false;
    assert e.config.surmount.managementUi.http3Enable == true;
    assert (e.config.services.splora.enable or false) == false;
    assert !(builtins.elem 443 (e.config.networking.firewall.allowedUDPPorts or [ ]));
    assert failedAssertions e == [ ];
    "t1-defaults-ok";

  # P1: Stalwart free-public-443 apply template is installed for operators;
  # product firewall SoT opens :443 for Axum edge; Stalwart openFirewall
  # default stays false (mail.nix mkDefault).
  t1b-p1-free-443-plan-and-firewall =
    let
      e = evalSurmount { };
      etc = e.config.environment.etc;
      planEntry = etc."surmount/stalwart/free-public-443-for-axum-edge.ndjson" or null;
      readmeEntry = etc."surmount/stalwart/README-free-public-443.txt" or null;
      planText =
        if planEntry == null then
          ""
        else if planEntry ? source then
          builtins.readFile planEntry.source
        else if planEntry ? text then
          planEntry.text
        else
          "";
      fw = e.config.networking.firewall.allowedTCPPorts;
    in
    assert planEntry != null;
    assert readmeEntry != null;
    assert lib.hasInfix "NetworkListener" planText;
    assert lib.hasInfix "https" planText;
    assert lib.hasInfix "127.0.0.1:8080" planText;
    assert lib.hasInfix "destroy" planText;
    assert builtins.elem 80 fw;
    assert builtins.elem 443 fw;
    assert builtins.elem 25 fw;
    assert e.config.services.stalwart.openFirewall == false;
    assert failedAssertions e == [ ];
    "t1b-p1-free-443-plan-and-firewall-ok";

  # Mailbox password hashing: host README pins Stalwart Argon2id default.
  t1c-mailbox-password-argon2id-readme =
    let
      e = evalSurmount { };
      etc = e.config.environment.etc;
      readmeEntry = etc."surmount/stalwart/README-mailbox-password-argon2id.txt" or null;
      readmeText =
        if readmeEntry == null then
          ""
        else if readmeEntry ? source then
          builtins.readFile readmeEntry.source
        else if readmeEntry ? text then
          readmeEntry.text
        else
          "";
    in
    assert readmeEntry != null;
    assert lib.hasInfix "argon2id" readmeText;
    assert lib.hasInfix "passwordHashAlgorithm" readmeText;
    assert lib.hasInfix "does not pre-hash" readmeText;
    assert failedAssertions e == [ ];
    "t1c-mailbox-password-argon2id-readme-ok";

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

  # Onion console surface: when Arti HS is enabled and management UI is on,
  # derive hostname file + HS state dir env (never invent a live .onion).
  t5b-arti-enable-derives-ui-onion-hostname-env =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = false;
        artiHiddenService.onionServiceStateDir = "/run/surmount-secrets/arti/onion-service";
        managementUi.enable = true;
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = lib.concatStringsSep "\n" envList;
      hsDir = e.config.surmount.artiHiddenService.onionServiceStateDir;
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "SURMOUNT_ONION_HOSTNAME_FILE=${hsDir}/hostname" envBlob;
    assert lib.hasInfix "SURMOUNT_ONION_HS_STATE_DIR=${hsDir}" envBlob;
    # No invented onion URL env when onionUrl option empty.
    assert !(lib.hasInfix "SURMOUNT_ONION_URL=" envBlob);
    "t5b-arti-enable-derives-ui-onion-hostname-env-ok";

  # Explicit managementUi.onionHostnameFile wins over Arti derive.
  t5c-onion-hostname-file-option-wins =
    let
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = false;
        managementUi.enable = true;
        managementUi.onionHostnameFile = "/run/surmount-secrets/arti/custom-hostname";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = lib.concatStringsSep "\n" envList;
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "SURMOUNT_ONION_HOSTNAME_FILE=/run/surmount-secrets/arti/custom-hostname"
      envBlob;
    assert
      !(lib.hasInfix "SURMOUNT_ONION_HOSTNAME_FILE=/run/surmount-secrets/arti/onion-service/hostname" envBlob);
    "t5c-onion-hostname-file-option-wins-ok";

  # Per-site v3 onions: two public HTTP Hosts get two different Arti nicknames.
  # Onion-Location must be each Host's own onion root (not /_o/{host} on a
  # shared onion). Mail Hosts stay unmapped. HS keys never appear in toml.
  t5d-per-site-onions-distinct-nicknames =
    let
      extraRoot = "/var/lib/surmount/static-sites/extra";
      e = evalSurmount {
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = false;
        managementUi.enable = true;
        managementUi.staticVhosts = {
          "extra.example.test" = {
            root = extraRoot;
          };
        };
        managementUi.extraMailHostnames = [ "mail.cryptoquick.com" ];
      };
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      nick = e.config.surmount.artiHiddenService.nickname;
      apex = e.config.surmount.primaryDomain;
      www = "www.${apex}";
      mail = e.config.surmount.mailHostname;
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = builtins.unsafeDiscardStringContext (lib.concatStringsSep "\n" envList);
      nickLine = lib.findFirst (s: lib.hasPrefix "SURMOUNT_ONION_SITE_NICKNAMES_FILE=" s) null envList;
      nickPath =
        if nickLine == null then "" else lib.removePrefix "SURMOUNT_ONION_SITE_NICKNAMES_FILE=" nickLine;
      nickEtc = e.config.environment.etc."surmount/onion-site-nicknames.json" or { };
      nickJson =
        if nickEtc ? text && nickEtc.text != null && nickEtc.text != "" then
          builtins.unsafeDiscardStringContext nickEtc.text
        else if nickEtc ? source && nickEtc.source != null then
          builtins.unsafeDiscardStringContext (builtins.readFile nickEtc.source)
        else
          "";
      nickMap = if nickJson == "" then { } else builtins.fromJSON nickJson;
      apexNick = nickMap.${apex} or "";
      wwwNick = nickMap.${www} or "";
      extraNick = nickMap."extra.example.test" or "";
      servicesNick = nickMap.${e.config.surmount.servicesHostname} or "";
    in
    assert failedAssertions e == [ ];
    assert e.config.environment.etc ? "surmount/onion-site-nicknames.json";
    assert lib.hasInfix "SURMOUNT_ONION_SITE_NICKNAMES_FILE=" envBlob;
    assert nickPath != "";
    assert nickJson != "";
    assert !(lib.hasInfix "/_o/" nickJson);
    assert !(lib.hasInfix "BEGIN PRIVATE" toml);
    assert !(lib.hasInfix "PRIVATE KEY" toml);
    assert !(lib.hasInfix "hs_ed25519_secret_key" toml);
    # Console keeps the existing HS nickname so the live identity stays that site.
    assert servicesNick == nick;
    assert lib.hasInfix "[onion_services.\"${nick}\"]" toml;
    # Two different public Hosts -> two different onion service nicknames.
    assert apexNick != "";
    assert wwwNick != "";
    assert extraNick != "";
    assert apexNick != wwwNick;
    assert extraNick != apexNick;
    assert extraNick != wwwNick;
    assert extraNick != nick;
    assert lib.hasInfix "[onion_services.\"${apexNick}\"]" toml;
    assert lib.hasInfix "[onion_services.\"${wwwNick}\"]" toml;
    assert lib.hasInfix "[onion_services.\"${extraNick}\"]" toml;
    # Mail Hosts stay unmapped (primary MX and extra mail hostname).
    assert !(nickMap ? ${mail});
    assert !(nickMap ? "mail.cryptoquick.com");
    assert !(lib.hasInfix "mail.cryptoquick.com" nickJson);
    assert !(lib.hasInfix "[onion_services.\"site-mail-" toml);
    "t5d-per-site-onions-distinct-nicknames-ok";

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
              system.stateVersion = "26.05";
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
              system.stateVersion = "26.05";
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
        # Public primary requires authMode=nostr (footgun guard).
        managementUi.authMode = "nostr";
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
        managementUi.authMode = "nostr";
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
        managementUi.authMode = "nostr";
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
        managementUi.authMode = "nostr";
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
    assert builtins.elem 443 (e.config.networking.firewall.allowedUDPPorts or [ ]);
    assert !(builtins.any (x: lib.hasPrefix "SURMOUNT_HTTP3=0" x) env);
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
        managementUi.authMode = "nostr";
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

  # Public primary listen + authMode=off is refused (open-console footgun guard).
  # Loopback auth-off remains OK; lab allowPublicAuthOff is the only escape.
  t10b-public-listen-auth-off-asserts =
    let
      ePublicOff = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.authMode = "off";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      eLoopbackOff = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "http";
        managementUi.listenAddress = "127.0.0.1";
        managementUi.port = 8090;
        managementUi.authMode = "off";
      };
      ePublicOffLab = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.authMode = "off";
        managementUi.allowPublicAuthOff = true;
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      ePublicNostr = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.authMode = "nostr";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
      };
      isAuthFootgun =
        a:
        lib.hasInfix "authMode" a.message
        && (
          lib.hasInfix "public" a.message || lib.hasInfix "loopback" a.message || lib.hasInfix "off" a.message
        );
      envLab = ePublicOffLab.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
    in
    assert failedAssertions ePublicOff != [ ];
    assert builtins.any isAuthFootgun (failedAssertions ePublicOff);
    assert failedAssertions eLoopbackOff == [ ];
    assert failedAssertions ePublicOffLab == [ ];
    assert builtins.any (x: x == "SURMOUNT_ALLOW_PUBLIC_AUTH_OFF=1") envLab;
    assert failedAssertions ePublicNostr == [ ];
    assert builtins.any (
      x: x == "SURMOUNT_AUTH_MODE=nostr"
    ) ePublicNostr.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
    "t10b-public-listen-auth-off-asserts-ok";

  # B4 public edge: allowlist file + session EnvironmentFile must be granted
  # (ReadOnlyPaths) and missing files must keep the unit inactive (not restart).
  t10c-nostr-allowlist-readonly-paths =
    let
      allowPath = "/var/lib/surmount/secrets/ui/nostr-allowlist";
      sessPath = "/var/lib/surmount/secrets/ui/session-secret";
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.listenAddress = "0.0.0.0";
        managementUi.port = 443;
        managementUi.authMode = "nostr";
        managementUi.tlsCertPath = "/var/lib/surmount/secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/var/lib/surmount/secrets/tls/key.pem";
        managementUi.sessionSecretPath = sessPath;
        managementUi.nostrAllowlistFile = allowPath;
        managementUi.publicBaseUrl = "https://services.example.test";
      };
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
      uc = e.config.systemd.services.surmount-management-ui.unitConfig;
      env = sc.Environment;
      ro =
        let
          p = sc.ReadOnlyPaths or [ ];
        in
        if builtins.isList p then p else [ p ];
      envFiles =
        let
          p = sc.EnvironmentFile or [ ];
        in
        if builtins.isList p then p else [ p ];
      cond =
        let
          p = uc.ConditionPathExists or [ ];
        in
        if builtins.isList p then p else [ p ];
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: x == "SURMOUNT_AUTH_MODE=nostr") env;
    assert builtins.any (x: x == "SURMOUNT_NOSTR_ALLOWLIST_FILE=${allowPath}") env;
    assert builtins.any (x: x == "SURMOUNT_PUBLIC_BASE_URL=https://services.example.test") env;
    assert builtins.elem allowPath ro;
    assert builtins.elem sessPath ro;
    assert builtins.elem sessPath envFiles;
    assert builtins.elem allowPath cond;
    assert builtins.elem sessPath cond;
    "t10c-nostr-allowlist-readonly-paths-ok";

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
        managementUi.authMode = "nostr";
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

  # Domain C Vaultwarden: enable without admin token path fails closed at eval.
  t22-vaultwarden-enable-needs-token-path =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "adminTokenEnvFile" a.message) failed;
    "t22-vaultwarden-enable-needs-token-path-ok";

  # Domain C: enable with host path wires stock VW, signups off, ConditionPathExists.
  t23-vaultwarden-enable-with-token-path =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        managementUi.enable = true;
      };
      svc = e.config.systemd.services.vaultwarden;
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = lib.concatStringsSep "\n" envList;
      vwCfg = e.config.services.vaultwarden.config;
    in
    assert failedAssertions e == [ ];
    assert e.config.services.vaultwarden.enable == true;
    assert e.config.services.vaultwarden.dbBackend == "sqlite";
    assert e.config.services.vaultwarden.configureNginx == false;
    assert vwCfg.SIGNUPS_ALLOWED == false;
    assert vwCfg.ROCKET_ADDRESS == "127.0.0.1";
    assert vwCfg.ROCKET_PORT == 8222;
    # Admin token path is EnvironmentFile (not inline secret).
    assert builtins.elem "/run/surmount-secrets/vaultwarden/admin.env" (
      if builtins.isList e.config.services.vaultwarden.environmentFile then
        e.config.services.vaultwarden.environmentFile
      else
        [ e.config.services.vaultwarden.environmentFile ]
    );
    # Missing host file => unit inactive (not restart thrash).
    assert svc.unitConfig.ConditionPathExists != null;
    assert lib.hasInfix "/run/surmount-secrets/vaultwarden/admin.env" (
      if builtins.isList svc.unitConfig.ConditionPathExists then
        lib.concatStringsSep " " svc.unitConfig.ConditionPathExists
      else
        toString svc.unitConfig.ConditionPathExists
    );
    # Console derives private loopback URL when vaultwardenUrl option empty.
    assert lib.hasInfix "SURMOUNT_VAULTWARDEN_URL=http://127.0.0.1:8222" envBlob;
    # Never put ADMIN_TOKEN value into UI Environment=.
    assert !(lib.hasInfix "ADMIN_TOKEN=" envBlob);
    "t23-vaultwarden-enable-with-token-path-ok";

  # Explicit managementUi.vaultwardenUrl wins over rocket derive.
  t24-vaultwarden-url-option-wins =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        managementUi.enable = true;
        managementUi.vaultwardenUrl = "https://vault.example.test";
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = lib.concatStringsSep "\n" envList;
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "SURMOUNT_VAULTWARDEN_URL=https://vault.example.test" envBlob;
    assert !(lib.hasInfix "SURMOUNT_VAULTWARDEN_URL=http://127.0.0.1:8222" envBlob);
    "t24-vaultwarden-url-option-wins-ok";

  # Bad admin token path charset fails closed.
  t25-vaultwarden-bad-token-path-asserts =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/tmp/evil;rm -rf /";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "strict" a.message || lib.hasInfix "adminTokenEnvFile" a.message
    ) failed;
    "t25-vaultwarden-bad-token-path-asserts-ok";

  # G1/S1: secret-bearing keys in extraConfig fail closed (store-bound env).
  t26-vaultwarden-extra-config-secret-key-asserts =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        vaultwarden.extraConfig.ADMIN_TOKEN = "not-a-real-token-eval-only";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "ADMIN_TOKEN" a.message
      || lib.hasInfix "secret-bearing" a.message
      || lib.hasInfix "extraConfig" a.message
    ) failed;
    "t26-vaultwarden-extra-config-secret-key-asserts-ok";

  # G3: extraConfig ROCKET_ADDRESS world bind is overridden to option loopback.
  t27-vaultwarden-force-rocket-after-extra-config =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        vaultwarden.extraConfig.ROCKET_ADDRESS = "0.0.0.0";
        vaultwarden.extraConfig.SIGNUPS_ALLOWED = true;
      };
      vwCfg = e.config.services.vaultwarden.config;
    in
    assert failedAssertions e == [ ];
    # Force after extraConfig: option default wins; signup stays false.
    assert vwCfg.ROCKET_ADDRESS == "127.0.0.1";
    assert vwCfg.SIGNUPS_ALLOWED == false;
    "t27-vaultwarden-force-rocket-after-extra-config-ok";

  # S2: rocketAddress world bind fails unless allowNonLoopbackListen.
  t28-vaultwarden-world-bind-asserts =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        vaultwarden.rocketAddress = "0.0.0.0";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "loopback" a.message
      || lib.hasInfix "rocketAddress" a.message
      || lib.hasInfix "allowNonLoopbackListen" a.message
    ) failed;
    "t28-vaultwarden-world-bind-asserts-ok";

  # S2 escape: deliberate non-loopback with allow flag succeeds.
  t29-vaultwarden-allow-non-loopback-ok =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        vaultwarden.rocketAddress = "0.0.0.0";
        vaultwarden.allowNonLoopbackListen = true;
      };
    in
    assert failedAssertions e == [ ];
    assert e.config.services.vaultwarden.config.ROCKET_ADDRESS == "0.0.0.0";
    "t29-vaultwarden-allow-non-loopback-ok";

  # S3: bad vaultwardenUrl charset/scheme fails when UI would publish env.
  t30-vaultwarden-bad-url-asserts =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.vaultwardenUrl = "javascript:alert(1)";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "vaultwardenUrl" a.message
      || lib.hasInfix "http" a.message
      || lib.hasInfix "charset" a.message
    ) failed;
    "t30-vaultwarden-bad-url-asserts-ok";

  # G5: configureNginx force true fails closed.
  t31-vaultwarden-configure-nginx-refused =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        __extraModules = [
          (
            { lib, ... }:
            {
              services.vaultwarden.configureNginx = lib.mkForce true;
            }
          )
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "configureNginx" a.message || lib.hasInfix "nginx" a.message
    ) failed;
    "t31-vaultwarden-configure-nginx-refused-ok";

  # G6: SMTP_PASSWORD in extraConfig also fails (second secret key).
  t32-vaultwarden-smtp-password-extra-asserts =
    let
      e = evalSurmount {
        vaultwarden.enable = true;
        vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
        vaultwarden.extraConfig.SMTP_PASSWORD = "not-a-real-password-eval-only";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "SMTP_PASSWORD" a.message || lib.hasInfix "secret-bearing" a.message
    ) failed;
    "t32-vaultwarden-smtp-password-extra-asserts-ok";

  # L2 Domain B: when in-process ACME is enabled, tmpfiles recreate writable
  # parents for cert/key/account under deploy material dir (no PEM contents).
  # ReadWritePaths covers those parents; ConditionPathExists on PEMs is skipped.
  t33-acme-enable-tmpfiles-and-write-paths =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.acme.enable = true;
        managementUi.acme.directory = "https://acme-staging-v02.api.letsencrypt.org/directory";
        managementUi.acme.email = "ops@example.test";
        managementUi.acme.domains = [ "services.example.test" ];
        managementUi.acme.accountCredentialsPath = "/run/surmount-secrets/acme/account.json";
        managementUi.acme.challenge = "dns-01";
        managementUi.acme.dnsProvider = "mock";
      };
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
      uc = e.config.systemd.services.surmount-management-ui.unitConfig;
      rules = e.config.systemd.tmpfiles.rules;
      rwp = sc.ReadWritePaths;
      env = sc.Environment;
      envList = if builtins.isList env then env else [ env ];
      hasRule = needle: builtins.any (r: lib.hasInfix needle r) rules;
    in
    assert failedAssertions e == [ ];
    # Durable /run parents for ACME write targets (owner surmount-ui).
    assert hasRule "d /run/surmount-secrets 0755 root root";
    assert hasRule "d /run/surmount-secrets/tls 0750 surmount-ui surmount-ui";
    assert hasRule "d /run/surmount-secrets/acme 0750 surmount-ui surmount-ui";
    # Parents on ReadWritePaths (create-on-issue under ProtectSystem=strict).
    assert builtins.elem "/run/surmount-secrets/tls" rwp;
    assert builtins.elem "/run/surmount-secrets/acme" rwp;
    # First-issue path: no ConditionPathExists on PEMs when ACME on.
    assert
      !(builtins.hasAttr "ConditionPathExists" uc)
      || uc.ConditionPathExists == null
      || uc.ConditionPathExists == [ ];
    assert
      builtins.any (x: x == "SURMOUNT_ACME_ENABLE=true" || x == "SURMOUNT_ACME_ENABLE=1") envList
      || builtins.any (x: lib.hasPrefix "SURMOUNT_ACME_" x) envList;
    "t33-acme-enable-tmpfiles-and-write-paths-ok";

  # external-hook defaults dnsHookPath to packaged Namecheap helper store path
  # (code only; no Namecheap secrets on the host).
  t33c-acme-external-hook-packaged-dns-hook =
    let
      e = evalSurmount {
        web.enable = false;
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.acme.enable = true;
        managementUi.acme.directory = "https://acme-staging-v02.api.letsencrypt.org/directory";
        managementUi.acme.email = "ops@example.test";
        managementUi.acme.domains = [ "services.example.test" ];
        managementUi.acme.accountCredentialsPath = "/run/surmount-secrets/acme/account.json";
        managementUi.acme.challenge = "dns-01";
        managementUi.acme.dnsProvider = "external-hook";
        # dnsHookPath intentionally unset: module mkDefault to package store path.
      };
      hook = e.config.surmount.managementUi.acme.dnsHookPath;
      sc = e.config.systemd.services.surmount-management-ui.serviceConfig;
      env = sc.Environment;
      envList = if builtins.isList env then env else [ env ];
    in
    assert failedAssertions e == [ ];
    assert hook != "";
    assert lib.hasInfix "acme-dns-hook-namecheap" hook;
    assert builtins.any (x: lib.hasPrefix "SURMOUNT_ACME_DNS_HOOK=" x) envList;
    "t33c-acme-external-hook-packaged-dns-hook-ok";

  # H3: recoveryAdminEnvFile path-only EnvironmentFile on stalwart-mail unit.
  t34-stalwart-recovery-envfile-path-only =
    let
      e = evalSurmount {
        __extraModules = [
          {
            services.stalwart.recoveryAdminEnvFile = "/var/lib/surmount/secrets/stalwart/recovery.env";
          }
        ];
      };
      sc = e.config.systemd.services.stalwart-mail.serviceConfig;
      ef = sc.EnvironmentFile or null;
      efList =
        if ef == null then
          [ ]
        else if builtins.isList ef then
          ef
        else
          [ ef ];
      env = e.config.systemd.services.stalwart-mail.environment;
    in
    assert failedAssertions e == [ ];
    assert
      e.config.services.stalwart.recoveryAdminEnvFile
      == "/var/lib/surmount/secrets/stalwart/recovery.env";
    # Path only with soft-missing prefix; never password body.
    assert builtins.any (p: lib.hasInfix "/var/lib/surmount/secrets/stalwart/recovery.env" p) efList;
    assert builtins.any (p: lib.hasPrefix "-" p) efList;
    assert !(builtins.any (p: lib.hasInfix "STALWART_RECOVERY_ADMIN=" p) efList);
    assert !(env ? STALWART_RECOVERY_ADMIN);
    "t34-stalwart-recovery-envfile-path-only-ok";

  # H3: explicit empty recoveryAdminEnvFile opts out of unit EnvironmentFile.
  t34b-stalwart-recovery-envfile-default-empty =
    let
      e = evalSurmount {
        __extraModules = [
          {
            services.stalwart.recoveryAdminEnvFile = "";
          }
        ];
      };
      sc = e.config.systemd.services.stalwart-mail.serviceConfig;
      hasEf =
        builtins.hasAttr "EnvironmentFile" sc && sc.EnvironmentFile != null && sc.EnvironmentFile != [ ];
    in
    assert failedAssertions e == [ ];
    assert e.config.services.stalwart.recoveryAdminEnvFile == "";
    assert !hasEf;
    "t34b-stalwart-recovery-envfile-default-empty-ok";

  # Recommended product default (mail.nix): durable Domain B recovery path.
  t34f-stalwart-recovery-recommended-durable-default =
    let
      e = evalSurmount { };
      sc = e.config.systemd.services.stalwart-mail.serviceConfig;
      ef = sc.EnvironmentFile or null;
      efList =
        if ef == null then
          [ ]
        else if builtins.isList ef then
          ef
        else
          [ ef ];
    in
    assert failedAssertions e == [ ];
    assert
      e.config.services.stalwart.recoveryAdminEnvFile
      == "/var/lib/surmount/secrets/stalwart/recovery.env";
    assert builtins.any (p: lib.hasInfix "/var/lib/surmount/secrets/stalwart/recovery.env" p) efList;
    assert builtins.any (p: lib.hasPrefix "-" p) efList;
    "t34f-stalwart-recovery-recommended-durable-default-ok";

  # Dual-sign File PEMs: mail unit must be able to read Domain B dkim dir
  # under ProtectSystem=strict.
  t35-stalwart-dkim-readonly-paths =
    let
      e = evalSurmount { };
      sc = e.config.systemd.services.stalwart-mail.serviceConfig;
      ro = sc.ReadOnlyPaths or [ ];
      roList = if builtins.isList ro then ro else [ ro ];
    in
    assert failedAssertions e == [ ];
    assert builtins.any (p: lib.hasInfix "/var/lib/surmount/secrets/mail/dkim" p) roList;
    "t35-stalwart-dkim-readonly-paths-ok";

  # Mail-plane TLS: Stalwart reads copies under secrets/mail/tls (0600
  # stalwart-mail). ProtectSystem=strict needs ReadOnlyPaths. Axum key is
  # 0600 owner-only; cert stays 0640 surmount-tls.
  t38-stalwart-mail-tls-pem-grant =
    let
      e = evalSurmount {
        managementUi.enable = true;
      };
      sc = e.config.systemd.services.stalwart-mail.serviceConfig;
      ro = sc.ReadOnlyPaths or [ ];
      roList = if builtins.isList ro then ro else [ ro ];
      supp = sc.SupplementaryGroups or [ ];
      suppList = if builtins.isList supp then supp else [ supp ];
      extra = e.config.users.users.stalwart-mail.extraGroups or [ ];
      uiExtra = e.config.users.users.surmount-ui.extraGroups or [ ];
      etc = e.config.environment.etc;
      planEntry = etc."surmount/stalwart/mail-plane-tls-le-pems.example.ndjson" or null;
      readmeEntry = etc."surmount/stalwart/README-mail-plane-tls.txt" or null;
      planText =
        if planEntry == null then
          ""
        else if planEntry ? source then
          builtins.readFile planEntry.source
        else if planEntry ? text then
          planEntry.text
        else
          "";
    in
    assert failedAssertions e == [ ];
    assert e.config.users.groups ? surmount-tls;
    assert builtins.any (p: lib.hasInfix "/var/lib/surmount/secrets/tls" p) roList;
    assert builtins.any (p: lib.hasInfix "/var/lib/surmount/secrets/mail/tls" p) roList;
    assert builtins.elem "surmount-tls" suppList;
    assert builtins.elem "surmount-tls" extra;
    assert builtins.elem "surmount-tls" uiExtra;
    assert planEntry != null;
    assert readmeEntry != null;
    assert lib.hasInfix "Certificate" planText;
    assert lib.hasInfix "/var/lib/surmount/secrets/mail/tls/cert.pem" planText;
    assert lib.hasInfix "/var/lib/surmount/secrets/mail/tls/key.pem" planText;
    assert lib.hasInfix "File" planText;
    assert builtins.any (
      r:
      lib.hasInfix "/tls/cert.pem" r
      && lib.hasInfix "0640" r
      && lib.hasInfix "surmount-tls" r
      && !(lib.hasInfix "mail/tls/cert.pem" r)
    ) e.config.systemd.tmpfiles.rules;
    assert builtins.any (
      r:
      lib.hasInfix "/tls/key.pem" r
      && lib.hasInfix "0600" r
      && lib.hasInfix "surmount-ui" r
      && !(lib.hasInfix "mail/tls/key.pem" r)
    ) e.config.systemd.tmpfiles.rules;
    assert builtins.any (
      r: lib.hasInfix "mail/tls/key.pem" r && lib.hasInfix "0600" r && lib.hasInfix "stalwart-mail" r
    ) e.config.systemd.tmpfiles.rules;
    "t38-stalwart-mail-tls-pem-grant-ok";

  # Host convenience: btop is on PATH when surmount.enable (SSH / just btop).
  t36-btop-system-package =
    let
      e = evalSurmount { };
      pkgsList = e.config.environment.systemPackages;
      isBtop =
        p:
        (p.pname or "") == "btop"
        || lib.hasPrefix "btop-" (p.name or "")
        || lib.hasSuffix "-btop" (p.name or "");
    in
    assert failedAssertions e == [ ];
    assert builtins.any isBtop pkgsList;
    "t36-btop-system-package-ok";

  # Host convenience: inxi is on PATH when surmount.enable (SSH / just host-inxi).
  t41-inxi-system-package =
    let
      e = evalSurmount { };
      pkgsList = e.config.environment.systemPackages;
      isInxi =
        p:
        (p.pname or "") == "inxi"
        || lib.hasPrefix "inxi-" (p.name or "")
        || lib.hasSuffix "-inxi" (p.name or "");
    in
    assert failedAssertions e == [ ];
    assert builtins.any isInxi pkgsList;
    "t41-inxi-system-package-ok";

  # Host convenience: shipped btop.conf matches the operator laptop look
  # (flat-remix by name, not the FHS theme path; no theme background).
  t37-btop-local-config =
    let
      e = evalSurmount { };
      etc = e.config.environment.etc;
      entry = etc."xdg/btop/btop.conf" or null;
      text =
        if entry == null then
          ""
        else if entry ? source then
          builtins.readFile entry.source
        else if entry ? text then
          entry.text
        else
          "";
      rules = e.config.systemd.tmpfiles.rules;
    in
    assert entry != null;
    assert lib.hasInfix "theme_background = false" text;
    assert lib.hasInfix "rounded_corners = false" text;
    assert lib.hasInfix ''graph_symbol = "block"'' text;
    assert lib.hasInfix ''color_theme = "flat-remix"'' text;
    assert !(lib.hasInfix "/usr/share/btop/themes/" text);
    assert builtins.any (r: lib.hasInfix "/root/.config/btop/btop.conf" r) rules;
    "t37-btop-local-config-ok";

  t42-guest-root-justfile =
    let
      e = evalSurmount { };
      entry = e.config.environment.etc."surmount/root-justfile" or null;
      text =
        if entry == null then
          ""
        else if entry ? source then
          builtins.readFile entry.source
        else if entry ? text then
          entry.text
        else
          "";
      rules = e.config.systemd.tmpfiles.rules;
      pkgsList = map (p: p.pname or p.name or "") e.config.environment.systemPackages;
    in
    assert entry != null;
    assert lib.hasInfix "nixbuilder_uid" text;
    assert lib.hasInfix "pane is dead" text;
    assert lib.hasInfix "runuser -u grok" text;
    assert lib.hasInfix "tmux attach -t grok-oss" text;
    assert lib.hasInfix "grok-oss running --json" text;
    assert builtins.any (r: lib.hasInfix "/root/justfile" r) rules;
    assert builtins.any (n: lib.hasPrefix "just" n) pkgsList;
    "t42-guest-root-justfile-ok";

  t43-scram-watch-highest-priority =
    let
      e = evalSurmount { };
      svc = e.config.systemd.services.surmount-scram or { };
      sc = svc.serviceConfig or { };
      wanted = svc.wantedBy or [ ];
    in
    assert sc.ExecStart or "" != "";
    assert lib.hasInfix "--watch" (toString (sc.ExecStart or ""));
    assert sc.Nice or 0 == -20;
    assert sc.OOMScoreAdjust or 0 == -1000;
    assert sc.CPUSchedulingPolicy or "" == "fifo";
    assert builtins.elem "multi-user.target" wanted;
    "t43-scram-watch-highest-priority-ok";

  t43b-swapfile-enable-without-path-asserts =
    let
      failed = failedAssertions (evalSurmount {
        swapFile.enable = true;
        swapFile.path = "";
      });
    in
    assert builtins.any (a: lib.hasInfix "swapFile" a.message) failed;
    "t43b-swapfile-enable-without-path-asserts-ok";

  serviceMemoryMax =
    e: name:
    let
      svc = e.config.systemd.services.${name} or { };
      sc = svc.serviceConfig or { };
    in
    sc.MemoryMax or null;

  serviceSlice =
    e: name:
    let
      svc = e.config.systemd.services.${name} or { };
      sc = svc.serviceConfig or { };
    in
    sc.Slice or null;

  serviceNice =
    e: name:
    let
      svc = e.config.systemd.services.${name} or { };
      sc = svc.serviceConfig or { };
    in
    sc.Nice or null;

  etcText =
    e: name:
    let
      entry = e.config.environment.etc.${name} or null;
    in
    if entry == null then
      ""
    else if entry ? source then
      builtins.readFile entry.source
    else if entry ? text then
      entry.text
    else
      "";

  # SHC ticket 261: ssh-ng builder path gets MemoryMax + about-95-percent
  # CPU/RAM/storage guards. Mail/critical units stay unstarved.
  # Lake is not part of this mail-host module.
  t39-remote-builder-enable-memory-cap =
    let
      e = evalSurmount {
        remoteBuilder.enable = true;
        managementUi.enable = true;
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = true;
        artiHiddenService.packageIsOnionServiceCapable = true;
      };
      slice = e.config.systemd.slices."surmount-builder".sliceConfig or { };
      userSliceName = "user-${toString e.config.users.users.nixbuilder.uid}";
      userSlice = e.config.systemd.slices.${userSliceName}.sliceConfig or { };
      helperSrc = toString (e.config.environment.etc."surmount/niced-builder".source or "");
      stdio = etcText e "surmount/niced-nix-daemon-stdio";
      trusted = e.config.nix.settings.extra-trusted-users or [ ];
      critical = [
        "stalwart-mail"
        "surmount-management-ui"
        "surmount-arti-hidden-service"
        "sshd"
      ];
      criticalNotCapped = builtins.all (
        name:
        let
          mem = serviceMemoryMax e name;
          sl = serviceSlice e name;
        in
        (mem == null || mem == "" || mem == "infinity")
        && (sl == null || sl != "surmount-builder.slice")
        && (serviceNice e name == null)
      ) critical;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.remoteBuilder.enable == true;
    assert e.config.users.users ? nixbuilder;
    assert e.config.users.users.nixbuilder.isNormalUser == true;
    assert !(builtins.elem "wheel" (e.config.users.users.nixbuilder.extraGroups or [ ]));
    assert builtins.elem "nixbuilder" trusted;
    assert slice.MemoryMax or null == "4G";
    assert !(slice ? CPUQuota);
    assert userSlice.MemoryMax or null == "4G";
    assert !(userSlice ? CPUQuota);
    assert (e.config.systemd.services.nix-daemon.serviceConfig.MemoryMax or null) == "4G";
    assert (e.config.systemd.services.nix-daemon.serviceConfig.Nice or null) == 19;
    assert e.config.nix.settings.max-jobs == 8;
    assert e.config.nix.settings.max-jobs != 64;
    assert e.config.environment.etc ? "surmount/niced-builder";
    assert e.config.environment.etc ? "surmount/niced-nix-daemon-stdio";
    assert !(e.config.environment.etc ? "surmount/niced-lake");
    assert !(e.config.systemd.services ? surmount-lake);
    assert !(e.config.programs.nix-ld.enable or false);
    assert lib.hasInfix "surmount-niced-builder" helperSrc;
    assert !(lib.hasInfix "lake" (lib.toLower helperSrc));
    assert lib.hasInfix "nix-daemon" stdio;
    assert lib.hasInfix "niced-builder" stdio;
    assert lib.hasInfix "MemoryMax" stdio;
    assert criticalNotCapped;
    assert serviceMemoryMax e "stalwart-mail" != "4G";
    assert serviceSlice e "stalwart-mail" != "surmount-builder.slice";
    assert serviceMemoryMax e "surmount-management-ui" != "4G";
    assert serviceSlice e "surmount-management-ui" != "surmount-builder.slice";
    assert serviceMemoryMax e "surmount-arti-hidden-service" != "4G";
    assert serviceNice e "stalwart-mail" == null;
    assert serviceNice e "surmount-management-ui" == null;
    assert serviceNice e "surmount-arti-hidden-service" == null;
    "t39-remote-builder-enable-memory-cap-ok";

  t39b-remote-builder-custom-budget-not-sku =
    let
      e = evalSurmount {
        remoteBuilder.enable = true;
        remoteBuilder.memoryMax = "1500M";
        remoteBuilder.cpuQuota = "95%";
        remoteBuilder.diskGuardPercent = 95;
      };
      slice = e.config.systemd.slices."surmount-builder".sliceConfig or { };
    in
    assert failedAssertions e == [ ];
    assert slice.MemoryMax or null == "1500M";
    assert slice.CPUQuota or null == "95%";
    assert !(e.config.systemd.services ? surmount-lake);
    # Scaffold / override must not bake a published guest RAM integer SKU.
    assert !(lib.hasInfix "65536" (e.config.surmount.remoteBuilder.memoryMax));
    assert serviceMemoryMax e "stalwart-mail" != "1500M";
    "t39b-remote-builder-custom-budget-not-sku-ok";

  t39c-remote-builder-off-no-builder-slice-cap =
    let
      e = evalSurmount { };
      hasBuilderSlice = e.config.systemd.slices ? "surmount-builder";
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.remoteBuilder.enable == false;
    assert !hasBuilderSlice;
    assert !(e.config.environment.etc ? "surmount/niced-builder");
    assert !(e.config.environment.etc ? "surmount/niced-nix-daemon-stdio");
    assert !(e.config.environment.etc ? "surmount/niced-lake");
    assert !(e.config.systemd.services ? surmount-lake);
    "t39c-remote-builder-off-no-builder-slice-cap-ok";

  # enable=true with empty MemoryMax is uncapped. Refuse at eval.
  t39d-remote-builder-empty-memory-max-asserts =
    let
      e = evalSurmount {
        remoteBuilder.enable = true;
        remoteBuilder.memoryMax = "";
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "memoryMax" a.message && lib.hasInfix "MemoryMax" a.message
    ) failed;
    "t39d-remote-builder-empty-memory-max-asserts-ok";

  # Overlay that introduces a Lake unit without surmount.lake.enable fails.
  t39e-remote-builder-refuses-lake-unit =
    let
      e = evalSurmount {
        remoteBuilder.enable = true;
        __extraModules = [
          {
            systemd.services.surmount-lake.wantedBy = lib.mkForce [ "multi-user.target" ];
          }
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "surmount-lake" a.message) failed;
    "t39e-remote-builder-refuses-lake-unit-ok";

  # remoteBuilder does not enable nix-ld (that was Lake-only).
  t39f-remote-builder-no-nix-ld =
    let
      e = evalSurmount {
        remoteBuilder.enable = true;
      };
    in
    assert failedAssertions e == [ ];
    assert !(e.config.programs.nix-ld.enable or false);
    assert !(e.config.environment.etc ? "surmount/niced-lake");
    assert !(e.config.systemd.services ? surmount-lake);
    "t39f-remote-builder-no-nix-ld-ok";

  # rustc lives under system nix-daemon on trusted ssh-ng. Cap that unit.
  t39g-nix-daemon-is-the-real-build-cgroup =
    let
      e = evalSurmount { remoteBuilder.enable = true; };
      d = e.config.systemd.services.nix-daemon.serviceConfig or { };
    in
    assert failedAssertions e == [ ];
    assert d.MemoryMax or null == e.config.surmount.remoteBuilder.memoryMax;
    assert d.Nice or null == 19;
    assert (d.IOSchedulingClass or "") == "idle";
    "t39g-nix-daemon-is-the-real-build-cgroup-ok";

  t39h-max-jobs-tracks-cores-not-fake-64 =
    let
      e = evalSurmount { remoteBuilder.enable = true; };
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.remoteBuilder.maxJobs == 8;
    assert e.config.nix.settings.max-jobs == 8;
    assert e.config.nix.settings.max-jobs != 64;
    "t39h-max-jobs-tracks-cores-not-fake-64-ok";

  t39o-builder-is-preferred-oom-victim =
    let
      e = evalSurmount { remoteBuilder.enable = true; };
      d = e.config.systemd.services.nix-daemon.serviceConfig or { };
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
    "t39o-builder-is-preferred-oom-victim-ok";

  # H3: bad recovery path charset fails closed.
  t34c-stalwart-recovery-envfile-bad-path-asserts =
    let
      e = evalSurmount {
        __extraModules = [
          {
            services.stalwart.recoveryAdminEnvFile = "/tmp/evil;rm -rf /";
          }
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "recoveryAdminEnvFile" a.message
      || lib.hasInfix "strict" a.message
      || lib.hasInfix "metacharacters" a.message
    ) failed;
    "t34c-stalwart-recovery-envfile-bad-path-asserts-ok";

  # H3: STALWART_RECOVERY_ADMIN in extraEnvironment fails closed (store secret ban).
  t34d-stalwart-recovery-extra-env-password-asserts =
    let
      e = evalSurmount {
        __extraModules = [
          {
            services.stalwart.extraEnvironment.STALWART_RECOVERY_ADMIN = "admin:not-a-real-password";
          }
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "STALWART_RECOVERY_ADMIN" a.message
      || lib.hasInfix "extraEnvironment" a.message
      || lib.hasInfix "recoveryAdminEnvFile" a.message
    ) failed;
    "t34d-stalwart-recovery-extra-env-password-asserts-ok";

  # H3: direct unit environment.STALWART_RECOVERY_ADMIN also fails closed.
  t34e-stalwart-recovery-unit-env-password-asserts =
    let
      e = evalSurmount {
        __extraModules = [
          {
            systemd.services.stalwart-mail.environment.STALWART_RECOVERY_ADMIN = "admin:not-a-real-password";
          }
        ];
      };
      failed = failedAssertions e;
    in
    assert failed != [ ];
    assert builtins.any (
      a:
      lib.hasInfix "STALWART_RECOVERY_ADMIN" a.message
      || lib.hasInfix "environment" a.message
      || lib.hasInfix "recoveryAdminEnvFile" a.message
    ) failed;
    "t34e-stalwart-recovery-unit-env-password-asserts-ok";

  # ACME off: no ACME parent tmpfiles under /run/surmount-secrets/tls (only stateDir/ui).
  t33b-acme-off-no-acme-tmpfiles =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        managementUi.acme.enable = false;
      };
      rules = e.config.systemd.tmpfiles.rules;
      hasTlsAcme = builtins.any (
        r: lib.hasInfix "/run/surmount-secrets/tls" r || lib.hasInfix "/run/surmount-secrets/acme" r
      ) rules;
    in
    assert failedAssertions e == [ ];
    assert !hasTlsAcme;
    "t33b-acme-off-no-acme-tmpfiles-ok";

  # Apex/www default document root is the packaged SurmountSystems/site store
  # path when the overlay provides pkgs.surmount-public-site.
  t40-apex-public-root-packaged-site =
    let
      e = evalSurmount { managementUi.enable = true; };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = builtins.unsafeDiscardStringContext (lib.concatStringsSep "\n" envList);
      root = builtins.unsafeDiscardStringContext (toString e.config.surmount.managementUi.apexPublicRoot);
      ro = e.config.systemd.services.surmount-management-ui.serviceConfig.ReadOnlyPaths or [ ];
      roList = map (p: builtins.unsafeDiscardStringContext (toString p)) (
        if builtins.isList ro then ro else [ ro ]
      );
    in
    assert failedAssertions e == [ ];
    assert root != "";
    assert lib.hasInfix "SURMOUNT_APEX_PUBLIC_ROOT=${root}" envBlob;
    assert builtins.any (p: p == root) roList;
    # Store path from overlay package, not the mutable /var/lib fallback.
    assert lib.hasPrefix "/nix/store/" root;
    assert !(lib.hasInfix "/var/lib/surmount/public-site" root);
    "t40-apex-public-root-packaged-site-ok";

  # Extra static Hosts: env JSON file + ReadOnlyPaths for extra roots.
  t41-static-vhosts-env-and-readonly-paths =
    let
      extraRoot = "/var/lib/surmount/static-sites/extra";
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.staticVhosts = {
          "extra.example.test" = {
            root = extraRoot;
          };
          "www.extra.example.test" = {
            root = extraRoot;
          };
        };
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = builtins.unsafeDiscardStringContext (lib.concatStringsSep "\n" envList);
      ro = e.config.systemd.services.surmount-management-ui.serviceConfig.ReadOnlyPaths or [ ];
      roList = map (p: builtins.unsafeDiscardStringContext (toString p)) (
        if builtins.isList ro then ro else [ ro ]
      );
      # Keep store context on the writeText path so eval can realize the JSON.
      fileLine = lib.findFirst (s: lib.hasPrefix "SURMOUNT_STATIC_VHOSTS_FILE=" s) null envList;
      jsonPath =
        if fileLine == null then "" else lib.removePrefix "SURMOUNT_STATIC_VHOSTS_FILE=" fileLine;
      jsonBody =
        if jsonPath == "" then "" else builtins.unsafeDiscardStringContext (builtins.readFile jsonPath);
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "SURMOUNT_STATIC_VHOSTS_FILE=" envBlob;
    assert jsonPath != "";
    assert lib.hasInfix "extra.example.test" jsonBody;
    assert lib.hasInfix extraRoot jsonBody;
    assert lib.hasInfix "www.extra.example.test" jsonBody;
    assert builtins.any (p: p == extraRoot) roList;
    "t41-static-vhosts-env-and-readonly-paths-ok";

  t41b-static-vhosts-default-proven-roots =
    let
      e = evalSurmount { managementUi.enable = true; };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = builtins.unsafeDiscardStringContext (lib.concatStringsSep "\n" envList);
      ro = e.config.systemd.services.surmount-management-ui.serviceConfig.ReadOnlyPaths or [ ];
      roList = map (p: builtins.unsafeDiscardStringContext (toString p)) (
        if builtins.isList ro then ro else [ ro ]
      );
      fileLine = lib.findFirst (s: lib.hasPrefix "SURMOUNT_STATIC_VHOSTS_FILE=" s) null envList;
      jsonPath =
        if fileLine == null then "" else lib.removePrefix "SURMOUNT_STATIC_VHOSTS_FILE=" fileLine;
      jsonBody =
        if jsonPath == "" then "" else builtins.unsafeDiscardStringContext (builtins.readFile jsonPath);
      keepRoots = [
        "/var/lib/surmount/static-sites/cryptoquick"
        "/var/lib/surmount/static-sites/baxterartworks"
        "/var/lib/surmount/static-sites/btcfur"
        "/var/lib/surmount/static-sites/exophiles"
        "/var/lib/surmount/static-sites/iantuckerstudios"
        "/var/lib/surmount/static-sites/nostrfurs"
        "/var/lib/surmount/static-sites/yiffa"
      ];
      dropRoots = [
        "/var/lib/surmount/static-sites/btcdragonlord"
        "/var/lib/surmount/static-sites/btckitties"
        "/var/lib/surmount/static-sites/denverspace"
        "/var/lib/surmount/static-sites/justsaybits"
      ];
      keepHosts = [
        "cryptoquick.com"
        "baxterartworks.com"
        "btcfur.com"
        "exophiles.org"
        "iantuckerstudios.com"
        "nostrfurs.com"
        "yiffa.app"
      ];
      dropHosts = [
        "btcdragonlord.com"
        "btckitties.com"
        "denver.space"
        "justsaybits.org"
      ];
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "SURMOUNT_STATIC_VHOSTS_FILE=" envBlob;
    assert jsonPath != "";
    assert lib.all (p: builtins.any (r: r == p) roList) keepRoots;
    assert lib.all (p: !(builtins.any (r: r == p) roList)) dropRoots;
    assert lib.all (h: lib.hasInfix h jsonBody) keepHosts;
    assert lib.all (h: !(lib.hasInfix h jsonBody)) dropHosts;
    "t41b-static-vhosts-default-proven-roots-ok";

  # Extra mail Hosts: default mail.cryptoquick.com when provenStaticVhosts
  # includes cryptoquick.com. Inventory env, not mail-domains.txt.
  t41c-extra-mail-hostnames-env =
    let
      e = evalSurmount { managementUi.enable = true; };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = builtins.unsafeDiscardStringContext (lib.concatStringsSep "\n" envList);
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.managementUi.extraMailHostnames == [ "mail.cryptoquick.com" ];
    assert builtins.any (x: x == "SURMOUNT_EXTRA_MAIL_HOSTNAMES=mail.cryptoquick.com") envList;
    assert lib.hasInfix "SURMOUNT_EXTRA_MAIL_HOSTNAMES=mail.cryptoquick.com" envBlob;
    "t41c-extra-mail-hostnames-env-ok";

  t42-journal-persistent-when-logging-on =
    let
      e = evalSurmount { };
      extra = e.config.services.journald.extraConfig or "";
      blob = builtins.replaceStrings [ "\n" ] [ " " ] extra;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.logging.enable == true;
    assert lib.hasInfix "Storage=persistent" blob;
    assert lib.hasInfix "SystemMaxUse=1G" blob;
    assert lib.hasInfix "RuntimeMaxUse=256M" blob;
    assert lib.hasInfix "RateLimitIntervalSec=30s" blob;
    assert lib.hasInfix "RateLimitBurst=50000" blob;
    assert lib.hasInfix "ForwardToSyslog=no" blob;
    assert (e.config.services.openssh.settings.LogLevel or "") == "VERBOSE";
    "t42-journal-persistent-when-logging-on-ok";

  t44-lake-default-off =
    let
      e = evalSurmount { };
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.lake.enable == false;
    assert !(e.config.systemd.services ? surmount-lake);
    assert !(e.config.systemd.slices ? "surmount-lake");
    "t44-lake-default-off-ok";

  t44b-lake-enable-memory-cap =
    let
      e = evalSurmount {
        lake.enable = true;
        lake.package = pkgs.writeShellScriptBin "lake" "exit 0";
      };
      s = e.config.systemd.services.surmount-lake.serviceConfig or { };
    in
    assert failedAssertions e == [ ];
    assert s.MemoryMax or null == "4G";
    assert s.Nice or null == 19;
    assert (s.OOMScoreAdjust or 0) > 0;
    assert builtins.elem "multi-user.target" (e.config.systemd.services.surmount-lake.wantedBy or [ ]);
    assert serviceNice e "stalwart-mail" == null;
    assert serviceNice e "sshd" == null;
    "t44b-lake-enable-memory-cap-ok";

  t45-grok-oss-default-off =
    let
      e = evalSurmount { };
      pkgsList = e.config.environment.systemPackages;
      isGrokOss =
        p:
        (p.pname or "") == "grok-oss"
        || (p.name or "") == "grok-oss"
        || lib.hasPrefix "grok-oss-" (p.name or "")
        || lib.hasSuffix "-grok-oss" (p.name or "");
      isTmux =
        p: (p.pname or "") == "tmux" || (p.name or "") == "tmux" || lib.hasPrefix "tmux-" (p.name or "");
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.grokOss.enable == false;
    assert e.config.surmount.grokOss.package == null;
    assert !(e.config.users.users ? grok);
    assert !(e.config.systemd.slices ? "user-1988");
    assert !(e.config.systemd.services ? surmount-grok-oss);
    assert !(builtins.any isGrokOss pkgsList);
    assert !(builtins.any isTmux pkgsList);
    "t45-grok-oss-default-off-ok";

  t45b-grok-oss-enable-user-and-memory-cap =
    let
      e = evalSurmount {
        grokOss.enable = true;
        grokOss.package = pkgs.writeShellScriptBin "grok-oss" "exit 0";
      };
      userSlice = e.config.systemd.slices."user-1988".sliceConfig or { };
      grok = e.config.users.users.grok or { };
      pkgsList = e.config.environment.systemPackages;
      isGrokOss =
        p:
        (p.pname or "") == "grok-oss"
        || (p.name or "") == "grok-oss"
        || lib.hasPrefix "grok-oss-" (p.name or "")
        || lib.hasSuffix "-grok-oss" (p.name or "");
      isTmux =
        p: (p.pname or "") == "tmux" || (p.name or "") == "tmux" || lib.hasPrefix "tmux-" (p.name or "");
      tmp = e.config.systemd.tmpfiles.rules or [ ];
    in
    assert failedAssertions e == [ ];
    assert e.config.users.users ? grok;
    assert grok.isNormalUser == true;
    assert grok.uid == 1988;
    assert grok.home == "/home/grok";
    assert grok.linger == false;
    assert userSlice.MemoryMax or null == "4G";
    assert !(userSlice ? Nice);
    assert !(e.config.systemd.services ? surmount-grok-oss);
    assert builtins.any isGrokOss pkgsList;
    assert builtins.any isTmux pkgsList;
    assert builtins.any (r: lib.hasInfix "/home/grok/.grok" r) tmp;
    assert serviceNice e "stalwart-mail" == null;
    assert serviceNice e "sshd" == null;
    "t45b-grok-oss-enable-user-and-memory-cap-ok";

  t45c-grok-oss-enable-requires-package =
    let
      failed = failedAssertions (evalSurmount {
        grokOss.enable = true;
      });
    in
    assert builtins.any (a: lib.hasInfix "grokOss.package" a.message) failed;
    "t45c-grok-oss-enable-requires-package-ok";

  t50-splora-proxy-default-off =
    let
      e = evalSurmount { managementUi.enable = true; };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.sploraProxy.enable == false;
    assert !(builtins.any (x: lib.hasPrefix "SURMOUNT_SPLORA_PROXY=1" x) envList);
    assert !(builtins.any (x: lib.hasPrefix "SURMOUNT_SPLORA_PORTAL_HOST=" x) envList);
    "t50-splora-proxy-default-off-ok";

  t51-splora-proxy-enable-env-and-udp-https =
    let
      e = evalSurmount {
        managementUi.enable = true;
        managementUi.listenMode = "https";
        managementUi.tlsCertPath = "/run/surmount-secrets/tls/cert.pem";
        managementUi.tlsKeyPath = "/run/surmount-secrets/tls/key.pem";
        sploraProxy.enable = true;
        sploraProxy.instances = {
          mainnet = {
            hosts = [ "esplora.example.test" ];
            socket = "/run/splora/mainnet.http.sock";
          };
        };
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      envBlob = builtins.unsafeDiscardStringContext (lib.concatStringsSep "\n" envList);
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: x == "SURMOUNT_SPLORA_PROXY=1") envList;
    assert builtins.any (x: x == "SURMOUNT_SPLORA_PORTAL_HOST=splora.surmount.systems") envList;
    assert lib.hasInfix "SURMOUNT_SPLORA_INSTANCES=" envBlob;
    assert lib.hasInfix ''"mainnet"'' envBlob;
    assert lib.hasInfix ''"hosts"'' envBlob;
    assert lib.hasInfix ''"socket"'' envBlob;
    assert builtins.elem 443 (e.config.networking.firewall.allowedUDPPorts or [ ]);
    assert e.config.surmount.managementUi.http3Enable == true;
    assert !(builtins.any (x: lib.hasPrefix "SURMOUNT_HTTP3=0" x) envList);
    assert !(builtins.elem "splora" (e.config.users.users.surmount-ui.extraGroups or [ ]));
    "t51-splora-proxy-enable-env-and-udp-https-ok";

  t52-splora-electrum-socket-asserts =
    let
      failed = failedAssertions (evalSurmount {
        managementUi.enable = true;
        sploraProxy.enable = true;
        sploraProxy.instances = {
          mainnet = {
            hosts = [ "esplora.example.test" ];
            socket = "/run/splora/mainnet.electrum.sock";
          };
        };
      });
    in
    assert failed != [ ];
    assert builtins.any (a: lib.hasInfix "Electrum" a.message) failed;
    "t52-splora-electrum-socket-asserts-ok";

  # Queue-only services.splora (empty instances: do not start five indexers).
  sploraQueueOnlyExtra = {
    services.splora.enable = true;
    services.splora.instances = { };
  };

  t53-splora-ui-extra-groups-when-both-on =
    let
      e = evalSurmount {
        managementUi.enable = true;
        sploraProxy.enable = true;
        sploraProxy.instances = {
          mainnet = {
            hosts = [ "esplora.example.test" ];
            socket = "/run/splora/mainnet.http.sock";
          };
        };
        __extraModules = [ sploraQueueOnlyExtra ];
      };
      extra = e.config.users.users.surmount-ui.extraGroups or [ ];
    in
    assert failedAssertions e == [ ];
    assert e.config.services.splora.enable == true;
    assert builtins.elem "splora" extra;
    assert e.config.surmount.managementUi.http3Enable == true;
    "t53-splora-ui-extra-groups-when-both-on-ok";

  t54-splora-ui-no-extra-group-when-service-off =
    let
      e = evalSurmount {
        managementUi.enable = true;
        sploraProxy.enable = true;
        sploraProxy.instances = {
          mainnet = {
            hosts = [ "esplora.example.test" ];
            socket = "/run/splora/mainnet.http.sock";
          };
        };
      };
    in
    assert failedAssertions e == [ ];
    assert (e.config.services.splora.enable or false) == false;
    assert !(builtins.elem "splora" (e.config.users.users.surmount-ui.extraGroups or [ ]));
    "t54-splora-ui-no-extra-group-when-service-off-ok";

  t55-splora-units-memory-max-when-set =
    let
      e = evalSurmount {
        managementUi.enable = true;
        sploraProxy.enable = true;
        sploraProxy.unitsMemoryMax = "2G";
        sploraProxy.instances = {
          mainnet = {
            hosts = [ "esplora.example.test" ];
            socket = "/run/splora/mainnet.http.sock";
          };
        };
        __extraModules = [ sploraQueueOnlyExtra ];
      };
      q = e.config.systemd.services.splora-queue.serviceConfig;
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.sploraProxy.unitsMemoryMax == "2G";
    assert q.MemoryMax or null == "2G";
    "t55-splora-units-memory-max-when-set-ok";

  t56-splora-units-memory-max-default-off =
    let
      e = evalSurmount {
        managementUi.enable = true;
        sploraProxy.enable = true;
        sploraProxy.instances = {
          mainnet = {
            hosts = [ "esplora.example.test" ];
            socket = "/run/splora/mainnet.http.sock";
          };
        };
        __extraModules = [ sploraQueueOnlyExtra ];
      };
      q = e.config.systemd.services.splora-queue.serviceConfig or { };
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.sploraProxy.unitsMemoryMax == null;
    assert !(q ? MemoryMax) || q.MemoryMax == null;
    "t56-splora-units-memory-max-default-off-ok";

  # Remote JSON-RPC wrap: one instance, cookie path, no local datadir.
  # Cookie bytes never appear. Imported instance daemonDir = null omits
  # --daemon-dir and does not list /var/lib/bitcoind on ReadOnlyPaths.
  sploraRemoteRpc = {
    enable = true;
    instanceName = "mainnet";
    network = "mainnet";
    jsonrpcImport = true;
    daemonRpcAddr = "127.0.0.1:8332";
    cookieFile = "/run/surmount-secrets/splora/rpc.cookie";
  };

  sploraReadOnlyBlob =
    rop:
    builtins.unsafeDiscardStringContext (
      if rop == null then
        ""
      else if builtins.isList rop then
        lib.concatStringsSep " " (map toString rop)
      else
        toString rop
    );

  t57-splora-remote-jsonrpc-no-local-datadir =
    let
      e = evalSurmount { sploraIndexer = sploraRemoteRpc; };
      svc = e.config.systemd.services.splora-mainnet or { };
      exec = builtins.unsafeDiscardStringContext (svc.serviceConfig.ExecStart or "");
      rop = sploraReadOnlyBlob (svc.serviceConfig.ReadOnlyPaths or null);
    in
    assert failedAssertions e == [ ];
    assert e.config.surmount.sploraIndexer.enable == true;
    assert e.config.services.splora.enable == true;
    assert (e.config.services.splora.instances.mainnet.enable or false) == true;
    assert e.config.systemd.services ? splora-mainnet;
    assert lib.hasInfix "--jsonrpc-import" exec;
    assert lib.hasInfix "--daemon-rpc-addr" exec;
    assert lib.hasInfix "127.0.0.1:8332" exec;
    assert lib.hasInfix "--cookie-file" exec;
    assert lib.hasInfix "/run/surmount-secrets/splora/rpc.cookie" exec;
    assert lib.hasInfix "--db-block-cache-mb" exec;
    assert lib.hasInfix "24" exec;
    assert !(lib.hasInfix "--daemon-dir" exec);
    assert !(lib.hasInfix "/var/lib/bitcoind" exec);
    assert !(lib.hasInfix "/var/lib/bitcoind" rop);
    assert (e.config.services.splora.instances.mainnet.daemonDir or "unset") == null;
    assert e.config.services.splora.instances.mainnet.jsonrpcImport == true;
    assert !(lib.hasInfix ":" e.config.surmount.sploraIndexer.cookieFile);
    "t57-splora-remote-jsonrpc-no-local-datadir-ok";

  t58-splora-remote-public-health-extra-arg =
    let
      e = evalSurmount {
        sploraIndexer = sploraRemoteRpc // {
          publicHealth = true;
        };
      };
      exec = builtins.unsafeDiscardStringContext (
        e.config.systemd.services.splora-mainnet.serviceConfig.ExecStart or ""
      );
    in
    assert failedAssertions e == [ ];
    assert lib.hasInfix "--public-health" exec;
    assert lib.hasInfix "--jsonrpc-import" exec;
    "t58-splora-remote-public-health-extra-arg-ok";

  t59-splora-remote-ui-extra-groups-when-proxy-on =
    let
      e = evalSurmount {
        managementUi.enable = true;
        sploraProxy.enable = true;
        sploraProxy.instances = {
          mainnet = {
            hosts = [ "esplora.example.test" ];
            socket = "/run/splora/mainnet.http.sock";
          };
        };
        sploraIndexer = sploraRemoteRpc;
      };
      extra = e.config.users.users.surmount-ui.extraGroups or [ ];
    in
    assert failedAssertions e == [ ];
    assert e.config.services.splora.enable == true;
    assert builtins.elem "splora" extra;
    assert e.config.surmount.managementUi.http3Enable == true;
    "t59-splora-remote-ui-extra-groups-when-proxy-on-ok";

  t60-splora-remote-enable-needs-cookie-path =
    let
      failed = failedAssertions (evalSurmount {
        sploraIndexer = {
          enable = true;
          daemonRpcAddr = "127.0.0.1:8332";
          cookieFile = "";
        };
      });
    in
    assert failed != [ ];
    assert builtins.any (
      a: lib.hasInfix "cookieFile" a.message || lib.hasInfix "cookie file" a.message
    ) failed;
    "t60-splora-remote-enable-needs-cookie-path-ok";

  # Portal Host is one DNS name (not a fifth indexer). Extra onion Hosts
  # include the portal and live indexer Hosts. Mainnet instance is not required.
  t61-splora-portal-host-and-onion-extras =
    let
      e = evalSurmount {
        managementUi.enable = true;
        artiHiddenService.enable = true;
        artiHiddenService.startDaemon = false;
        sploraProxy.enable = true;
        sploraProxy.instances = {
          testnet3.hosts = [ "testnet3.esplora.surmount.systems" ];
          testnet4.hosts = [ "testnet4.esplora.surmount.systems" ];
          mutinynet.hosts = [ "mutinynet.esplora.surmount.systems" ];
          liquid.hosts = [ "liquid.esplora.surmount.systems" ];
        };
      };
      env = e.config.systemd.services.surmount-management-ui.serviceConfig.Environment;
      envList = if builtins.isList env then env else [ env ];
      nickEtc = e.config.environment.etc."surmount/onion-site-nicknames.json" or { };
      nickJson =
        if nickEtc ? text && nickEtc.text != null && nickEtc.text != "" then
          builtins.unsafeDiscardStringContext nickEtc.text
        else if nickEtc ? source && nickEtc.source != null then
          builtins.unsafeDiscardStringContext (builtins.readFile nickEtc.source)
        else
          "";
      nickMap = if nickJson == "" then { } else builtins.fromJSON nickJson;
      toml = builtins.readFile e.config.environment.etc."surmount/arti.toml".source;
      portalNick = nickMap."splora.surmount.systems" or "";
    in
    assert failedAssertions e == [ ];
    assert builtins.any (x: x == "SURMOUNT_SPLORA_PORTAL_HOST=splora.surmount.systems") envList;
    assert nickMap ? "splora.surmount.systems";
    assert nickMap ? "testnet3.esplora.surmount.systems";
    assert nickMap ? "testnet4.esplora.surmount.systems";
    assert nickMap ? "mutinynet.esplora.surmount.systems";
    assert nickMap ? "liquid.esplora.surmount.systems";
    assert !(nickMap ? "esplora.surmount.systems");
    assert portalNick != "";
    assert lib.hasInfix "[onion_services.\"${portalNick}\"]" toml;
    "t61-splora-portal-host-and-onion-extras-ok";

  results = [
    t1-defaults
    t1b-p1-free-443-plan-and-firewall
    t1c-mailbox-password-argon2id-readme
    t2-require-empty-paths-asserts
    t3-bad-path-charset-asserts
    t4-require-paths-wires-check
    t5-arti-enable-no-daemon
    t5b-arti-enable-derives-ui-onion-hostname-env
    t5c-onion-hostname-file-option-wins
    t5d-per-site-onions-distinct-nicknames
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
    t10b-public-listen-auth-off-asserts
    t10c-nostr-allowlist-readonly-paths
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
    t22-vaultwarden-enable-needs-token-path
    t23-vaultwarden-enable-with-token-path
    t24-vaultwarden-url-option-wins
    t25-vaultwarden-bad-token-path-asserts
    t26-vaultwarden-extra-config-secret-key-asserts
    t27-vaultwarden-force-rocket-after-extra-config
    t28-vaultwarden-world-bind-asserts
    t29-vaultwarden-allow-non-loopback-ok
    t30-vaultwarden-bad-url-asserts
    t31-vaultwarden-configure-nginx-refused
    t32-vaultwarden-smtp-password-extra-asserts
    t33-acme-enable-tmpfiles-and-write-paths
    t33c-acme-external-hook-packaged-dns-hook
    t33b-acme-off-no-acme-tmpfiles
    t34-stalwart-recovery-envfile-path-only
    t34b-stalwart-recovery-envfile-default-empty
    t34c-stalwart-recovery-envfile-bad-path-asserts
    t34d-stalwart-recovery-extra-env-password-asserts
    t34e-stalwart-recovery-unit-env-password-asserts
    t34f-stalwart-recovery-recommended-durable-default
    t35-stalwart-dkim-readonly-paths
    t38-stalwart-mail-tls-pem-grant
    t36-btop-system-package
    t41-inxi-system-package
    t37-btop-local-config
    t42-guest-root-justfile
    t43-scram-watch-highest-priority
    t43b-swapfile-enable-without-path-asserts
    t39-remote-builder-enable-memory-cap
    t39b-remote-builder-custom-budget-not-sku
    t39c-remote-builder-off-no-builder-slice-cap
    t39d-remote-builder-empty-memory-max-asserts
    t39e-remote-builder-refuses-lake-unit
    t39f-remote-builder-no-nix-ld
    t39g-nix-daemon-is-the-real-build-cgroup
    t39h-max-jobs-tracks-cores-not-fake-64
    t39o-builder-is-preferred-oom-victim
    t40-apex-public-root-packaged-site
    t41-static-vhosts-env-and-readonly-paths
    t41b-static-vhosts-default-proven-roots
    t41c-extra-mail-hostnames-env
    t42-journal-persistent-when-logging-on
    t44-lake-default-off
    t44b-lake-enable-memory-cap
    t45-grok-oss-default-off
    t45b-grok-oss-enable-user-and-memory-cap
    t45c-grok-oss-enable-requires-package
    t50-splora-proxy-default-off
    t51-splora-proxy-enable-env-and-udp-https
    t52-splora-electrum-socket-asserts
    t53-splora-ui-extra-groups-when-both-on
    t54-splora-ui-no-extra-group-when-service-off
    t55-splora-units-memory-max-when-set
    t56-splora-units-memory-max-default-off
    t57-splora-remote-jsonrpc-no-local-datadir
    t58-splora-remote-public-health-extra-arg
    t59-splora-remote-ui-extra-groups-when-proxy-on
    t60-splora-remote-enable-needs-cookie-path
    t61-splora-portal-host-and-onion-extras
  ];
in
{
  inherit results;
  ok = results;
  inherit t41b-static-vhosts-default-proven-roots;
  inherit
    t53-splora-ui-extra-groups-when-both-on
    t57-splora-remote-jsonrpc-no-local-datadir
    t58-splora-remote-public-health-extra-arg
    t59-splora-remote-ui-extra-groups-when-proxy-on
    t60-splora-remote-enable-needs-cookie-path
    ;
}
