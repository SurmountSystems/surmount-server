# Arti onion / hidden service (REQUIRED product surface).
#
# Operator direction 2026-07-30: services must be reachable via Arti HS
# alongside clearnet. This module generates a management-publish arti.toml
# (onion service + reverse-proxy to management UI) without private key material.
#
# Absolute: HS private keys and identity state are deploy secrets on the
# host only. Never put keys (plain or ciphertext) in git or module examples.
#
# Default lean surfaces (Q-ARTI-3): management HTTP only.
# Do NOT onion-publish Stalwart admin/JMAP unless explicitly enabled.
# publishStalwartAdmin/Jmap flags are reserved; they do not yet add stanzas
# (no backend address options). Defaults stay false.
#
# Daemon start is OFF by default (startDaemon=false). enable=true installs
# config/docs only. When startDaemon=true with the complete management-publish
# config (default lean path), acceptIncompleteOnionConfig is NOT required and
# has no effect in this module version. startDaemon requires a service-capable
# package gate: Surmount artiOnionService passthru claim (null package happy
# path) or explicit packageIsOnionServiceCapable=true. Stock nixpkgs arti is
# client-default; unit active != onion published without a capable binary.
#
# Lean onion backend expects cleartext HTTP (or UDS) on the local target.
# When managementUi.listenMode=https and no explicit backend, the module
# auto-points at managementUi local cleartext (loopback API bind). Pointing
# cleartext Arti at the primary https TCP without that path still warns.
#
# Version assumption: arti.toml shape matches Surmount-owned Arti 2.5.1
# (nix/packages/arti-onion-service.nix; not stock nixpkgs 1.4.x lag). Keys:
# [storage] cache_dir/state_dir, [onion_services."<nickname>"] proxy_ports,
# [proxy] socks_listen (not legacy socks_port). See upstream
# crates/arti/src/arti-example-config.toml and doc/OnionService.md.
#
# Packaging: Surmount flake exposes pkgs.artiOnionService (distinct from stock
# pkgs.arti): current upstream source build + cargo feature
# onion-service-service. When package is null, this module prefers
# artiOnionService if present and treats passthru.surmountOnionServiceCapable
# as the capable claim. Stock pkgs.arti stays client-default; pointing package
# at stock without packageIsOnionServiceCapable=true fails closed (documented
# footgun if the bool is forced true on a client-only binary). Live Tor
# publish still residual.
#
# Research: docs/research/arti-and-secrets-manager.md
# Living: docs/COMPACTION-PIN.md section 7, docs/EDGE_AND_TLS.md

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  hs = cfg.artiHiddenService;
  ui = cfg.managementUi;
  inherit (lib) mkIf;

  hostPaths = import ./lib/host-paths.nix { inherit lib; };
  localCleartext = import ./lib/local-cleartext.nix { inherit lib; };

  # Prefer Surmount service-capable package when package is null so the happy
  # path is enable + host HS secrets (not silent stock client arti).
  pkg =
    if hs.package != null then
      hs.package
    else if (pkgs ? artiOnionService) && pkgs.artiOnionService != null then
      pkgs.artiOnionService
    else if (pkgs ? arti) && pkgs.arti != null then
      pkgs.arti
    else
      null;

  # Surmount artiOnionService sets passthru.surmountOnionServiceCapable.
  # Real derivations also surface passthru attrs at the top level.
  packageClaimsOnionServiceCapable =
    pkg != null
    && (
      (pkg.passthru.surmountOnionServiceCapable or false) || (pkg.surmountOnionServiceCapable or false)
    );

  # Operator bool OR Surmount-marked package. Stock path stays fail-closed.
  onionServiceCapable = hs.packageIsOnionServiceCapable || packageClaimsOnionServiceCapable;

  # Safe for unit strings when package missing (assertion fails closed).
  artiBin = if pkg != null then "${pkg}/bin/arti" else "arti-package-missing";

  # Default TCP target tracks managementUi bind so port/address drift cannot
  # silently leave the onion pointing at a dead address.
  derivedUiTcp = "${ui.listenAddress}:${toString ui.port}";

  # When UI is https and no explicit backend, prefer the local cleartext API
  # (auto or explicit localCleartextListen) so Arti never hits TLS-only TCP.
  effectiveLocalCleartext = localCleartext.effectiveLocalCleartextListen {
    inherit ui hs;
  };

  # tor-hsrproxy target: bare host:port for TCP; unix:/path for UDS.
  # UDS parse is accepted upstream; full UDS forward still has upstream gaps
  # (tor-hsrproxy TODO #1246). Prefer TCP loopback for production lean path.
  proxyTarget =
    if hs.backendUnixSocket != null then
      "unix:${hs.backendUnixSocket}"
    else if hs.backendAddress != null then
      hs.backendAddress
    else if effectiveLocalCleartext != null then
      effectiveLocalCleartext
    else
      derivedUiTcp;

  # True when the onion TCP target is the primary management UI listen (TLS risk
  # when listenMode=https). Auto cleartext / explicit backend / UDS are false.
  proxiesToManagementUiTcp = hs.backendUnixSocket == null && (proxyTarget == derivedUiTcp);

  # backendAddress charset when explicitly set (host:port or [v6]:port).
  # Nix builtins.match is POSIX ERE (not PCRE); [[] matches a literal '['.
  backendAddressOk =
    hs.backendAddress == null
    || builtins.match "[A-Za-z0-9._-]+:[0-9]+" hs.backendAddress != null
    || builtins.match "[[][0-9a-fA-F:]+[]]:[0-9]+" hs.backendAddress != null;

  # Complete management-publish config. No private keys or .onion addresses.
  # state_dir points at onionServiceStateDir so HS identity + HS instance state
  # live under the deploy-secrets host path. cache_dir stays under stateDir.
  artiConfig = pkgs.writeText "surmount-arti.toml" ''
    # Surmount Arti HS management-publish config (generated).
    # No private keys in this file. Onion service identity lives under
    # onionServiceStateDir on the host (deploy secrets; never in git).
    #
    # Version assumption: Surmount-owned Arti 2.5.1 config shape
    # (nix/packages/arti-onion-service.nix; GitLab arti-v2.5.1). Not stock
    # nixpkgs 1.4.x lag. Keys: socks_listen, onion_services, proxy_ports.
    # Command: arti proxy -c /etc/surmount/arti.toml
    #
    # NOTE: systemd unit active does not prove an onion is published.
    # Prefer pkgs.artiOnionService (Surmount overlay). Stock pkgs.arti is
    # client-default. Live Tor verify + operator HS keys remain residual.

    [application]
    watch_configuration = false

    [proxy]
    # HS publisher only; no local SOCKS egress required for lean management onion.
    socks_listen = 0

    [storage]
    cache_dir = "${hs.stateDir}/cache"
    # HS identity + HS instance state (operator-placed deploy secrets root).
    state_dir = "${hs.onionServiceStateDir}"

    [storage.permissions]
    trust_user = ":current"

    # stdout -> journald via the unit. Never log HS keys.
    [logging]
    console = "info"
    log_sensitive_information = false

    # Lean default: one onion service reverse-proxying to management UI.
    # Backend is cleartext TCP or unix: (not TLS). Stalwart admin/JMAP publish
    # options default false and do not add stanzas yet.
    [onion_services."${hs.nickname}"]
    proxy_ports = [
        ["80", "${proxyTarget}"],
        ["*", "destroy"]
    ]
  '';
in
{
  config = mkIf (cfg.enable && hs.enable) {
    assertions = [
      {
        assertion = !hs.startDaemon || pkg != null;
        message = ''
          surmount.artiHiddenService.startDaemon is true but no Arti package is
          available (pkgs.arti missing and surmount.artiHiddenService.package
          is null). Add an overlay package or set package explicitly.
        '';
      }
      {
        # Fail closed: stock channel arti is client-default. Unit active with a
        # client-only binary is not "onion published." Need Surmount
        # artiOnionService (passthru claim) or an explicit operator capable claim.
        assertion = !hs.startDaemon || onionServiceCapable;
        message = ''
          surmount.artiHiddenService.startDaemon is true but the selected arti
          package is not marked onion-service capable (packageIsOnionServiceCapable
          is false and package lacks passthru.surmountOnionServiceCapable).
          Stock nixpkgs arti builds client features only (no onion-service-service).
          A process that stays active is not proof an onion is published.
          Happy path: leave package null so the module uses pkgs.artiOnionService
          from the Surmount overlay, or set package = pkgs.artiOnionService.
          For a non-Surmount binary, set packageIsOnionServiceCapable = true only
          when that binary was built with onion-service-service (footgun if lied).
          Or leave startDaemon false.
        '';
      }
      {
        assertion = hostPaths.strictHostPath hs.onionServiceStateDir;
        message = ''
          surmount.artiHiddenService.onionServiceStateDir must be a strict
          absolute host path for HS identity (deploy secrets). Never a path
          inside the public repository.
        '';
      }
      {
        assertion = hostPaths.strictHostPath hs.stateDir;
        message = "surmount.artiHiddenService.stateDir must be a strict absolute host path.";
      }
      {
        assertion = hs.backendUnixSocket == null || hostPaths.strictHostPath hs.backendUnixSocket;
        message = "surmount.artiHiddenService.backendUnixSocket must be a strict absolute path when set.";
      }
      {
        assertion = backendAddressOk;
        message = ''
          surmount.artiHiddenService.backendAddress must be host:port or
          [ipv6]:port (simple charset) when set. Example: 127.0.0.1:8090.
        '';
      }
      {
        assertion =
          !(
            lib.hasInfix "BEGIN" hs.onionServiceStateDir
            || lib.hasInfix "PRIVATE KEY" hs.onionServiceStateDir
            || lib.hasInfix "BEGIN" hs.stateDir
          );
        message = ''
          artiHiddenService paths must not embed key material. Use host
          directory paths only (e.g. /run/surmount-secrets/arti/onion-service).
        '';
      }
      {
        # Nickname is an Arti local name embedded in TOML table keys.
        assertion = builtins.match "[A-Za-z0-9][A-Za-z0-9_-]*" hs.nickname != null;
        message = ''
          surmount.artiHiddenService.nickname must be a simple Arti HS nickname
          (start with alphanumeric; then alphanumeric, underscore, or hyphen).
        '';
      }
    ];

    warnings =
      lib.optional
        (ui.enable && ui.listenMode == "https" && !ui.allowCleartextHttpsEscape && proxiesToManagementUiTcp)
        ''
          surmount.artiHiddenService reverse-proxies cleartext TCP to the management
          UI primary listen (${derivedUiTcp}), but managementUi.listenMode is
          "https" (TLS). Arti will not speak TLS to that backend. Happy path:
          leave backendAddress/backendUnixSocket null so the module auto-binds
          a loopback cleartext API (localCleartextListen) and points Arti there,
          or set an explicit plain-HTTP backendAddress/UDS, or use
          listenMode=http for the local target. Do not invent TLS-on-onion.
        '';

    users.groups.surmount-arti = { };
    users.users.surmount-arti = {
      isSystemUser = true;
      group = "surmount-arti";
      description = "Surmount Arti hidden service";
    };

    environment.etc."surmount/arti.toml".source = artiConfig;

    # Process cache under stateDir. HS state dir is operator deploy secrets:
    # do not auto-create onionServiceStateDir (ConditionPathIsDirectory gates daemon).
    systemd.tmpfiles.rules = [
      "d ${hs.stateDir} 0750 surmount-arti surmount-arti - -"
      "d ${hs.stateDir}/cache 0750 surmount-arti surmount-arti - -"
    ];

    # Oneshot marker: enable=true without startDaemon does not touch multi-user
    # health beyond a successful oneshot.
    systemd.services.surmount-arti-scaffold-status = mkIf (!hs.startDaemon) {
      description = "Surmount Arti HS config status (daemon not started)";
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = pkgs.writeShellScript "surmount-arti-status" ''
          set -eu
          echo "surmount: arti HS module enabled (management-publish config installed; startDaemon=false)."
          echo "surmount: onionServiceStateDir expected at operator host path (never in git)."
          echo "surmount: must be owned/writable by surmount-arti (e.g. 0750 surmount-arti:surmount-arti)."
          echo "surmount: do not auto-create HS identity dir; place keys on host only."
          echo "surmount: unit active != onion published; need service-capable arti (artiOnionService) + live Tor verify."
          echo "surmount: set startDaemon=true only when HS state dir exists and package can run onion services."
        '';
      };
    };

    # Real daemon: startDaemon=true. Complete management-publish config does
    # not require acceptIncompleteOnionConfig (flag has no effect this version).
    # Requires HS state dir present via ConditionPathIsDirectory (no start,
    # no crash-loop when deploy secrets missing).
    systemd.services.surmount-arti-hidden-service = mkIf hs.startDaemon {
      description = "Surmount Arti onion/hidden service";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network-online.target"
        "surmount-management-ui.service"
      ];
      wants = [ "network-online.target" ];
      # Fail closed if HS identity dir is missing (deploy secrets on host).
      # Condition failure => inactive (dead), not restart loop.
      # NOTE: existence only; ownership must allow User=surmount-arti to write
      # keystore/state or the unit will fail at runtime (restart budget applies).
      unitConfig = {
        ConditionPathIsDirectory = hs.onionServiceStateDir;
        # Cap restarts if binary rejects config or perms fail so multi-user
        # is not wedged forever. Does not detect client-only silent success;
        # packageIsOnionServiceCapable gates that at eval.
        StartLimitIntervalSec = 300;
        StartLimitBurst = 5;
      };

      serviceConfig = {
        Type = "simple";
        User = "surmount-arti";
        Group = "surmount-arti";
        ExecStart = "${artiBin} proxy -c /etc/surmount/arti.toml";
        Restart = "on-failure";
        RestartSec = "10s";
        # state_dir is onionServiceStateDir (HS identity + instance state).
        # cache_dir is under stateDir. Both need write for a live publisher.
        ReadWritePaths = [
          hs.stateDir
          hs.onionServiceStateDir
        ];
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        SystemCallArchitectures = "native";
        # Hardening subset is intentional lean for this slice. Align further
        # with management-ui (MDWE, namespaces, etc.) only after a real
        # `arti proxy` smoke under the sandbox (residual; do not claim unsafe
        # without a failed run).
      };
    };

    # Marker file for ops/tests. onionServiceCapable = effective gate (option
    # bool OR Surmount passthru claim). packageIsOnionServiceCapable = raw
    # Nix option only. packageClaimsOnionServiceCapable = passthru on package.
    environment.etc."surmount/arti-backend-target".text = ''
      nickname=${hs.nickname}
      backend=${proxyTarget}
      onionServiceStateDir=${hs.onionServiceStateDir}
      stateDir=${hs.stateDir}
      startDaemon=${if hs.startDaemon then "true" else "false"}
      acceptIncompleteOnionConfig=${if hs.acceptIncompleteOnionConfig then "true" else "false"}
      onionServiceCapable=${if onionServiceCapable then "true" else "false"}
      packageIsOnionServiceCapable=${if hs.packageIsOnionServiceCapable then "true" else "false"}
      packageClaimsOnionServiceCapable=${if packageClaimsOnionServiceCapable then "true" else "false"}
      publishStalwartAdmin=${if hs.publishStalwartAdmin then "true" else "false"}
      publishStalwartJmap=${if hs.publishStalwartJmap then "true" else "false"}
      package=${if pkg != null then pkg.pname or "arti" else "missing"}
      configKind=management-publish
    '';
  };
}
