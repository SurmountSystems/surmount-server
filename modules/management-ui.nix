# Packages and runs the Rust management UI behind systemd.
# Product public HTTPS: this binary (listenMode=https + host PEMs) when
# surmount.web.enable is false (default). Dual-run escape: web.enable=true
# + listenMode=http behind transitional nginx (modules/web.nix).
# Optional plain HTTP redirect-only listener when redirectHttpToHttps is on
# (not while web.enable owns :80). Optional loopback cleartext full API
# (localCleartextListen / auto when https+Arti) for onion rproxy lean path.
# See docs/EDGE_AND_TLS.md, RESIDUAL.md, modules/lib/local-cleartext.nix.
#
# listenMode=https uses in-process rustls (TLS 1.3) with host PEM paths.
# allowCleartextHttpsEscape is emergency-only (cleartext under https).

{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.surmount;
  inherit (lib) mkIf optional;
  hostPaths = import ./lib/host-paths.nix { inherit lib; };
  localCleartext = import ./lib/local-cleartext.nix { inherit lib; };
  pkg =
    if cfg.managementUi.package != null then cfg.managementUi.package else pkgs.surmount-management-ui;

  ui = cfg.managementUi;
  hs = cfg.artiHiddenService;
  ac = cfg.accessControl;

  redirectListenActive = ui.redirectHttpToHttps && ui.httpRedirectListen != "";

  # Loopback cleartext full API for Arti (or explicit localCleartextListen).
  # Not the redirect-only :80 listener. See modules/lib/local-cleartext.nix.
  effectiveLocalCleartext = localCleartext.effectiveLocalCleartextListen {
    inherit ui hs;
  };
  localCleartextActive = effectiveLocalCleartext != null;

  # Ban env only when accessControl.enable (defaults stay off in binary otherwise).
  banStatePath =
    if ac.statePath != "" then
      ac.statePath
    else if ac.enable then
      "${cfg.stateDir}/ui/ban-state.json"
    else
      "";

  banWhitelistCsv = lib.concatStringsSep "," ac.whitelist;

  envList = [
    "SURMOUNT_LISTEN=${ui.listenAddress}:${toString ui.port}"
    "SURMOUNT_LISTEN_MODE=${ui.listenMode}"
    "SURMOUNT_PRIMARY_DOMAIN=${cfg.primaryDomain}"
    "SURMOUNT_MAIL_HOSTNAME=${cfg.mailHostname}"
    "SURMOUNT_SERVICES_HOSTNAME=${cfg.servicesHostname}"
    # Stalwart 0.16 default HTTP management listener.
    "SURMOUNT_STALWART_URL=http://127.0.0.1:8080"
    "SURMOUNT_REDIRECT_HTTP_TO_HTTPS=${if ui.redirectHttpToHttps then "true" else "false"}"
    "SURMOUNT_RATE_LIMIT_MAX=${toString ui.rateLimitMaxRequests}"
    "SURMOUNT_RATE_LIMIT_WINDOW_SECS=${toString ui.rateLimitWindowSecs}"
    "SURMOUNT_RATE_LIMIT_MAX_KEYS=${toString ui.rateLimitMaxKeys}"
    "RUST_LOG=info,surmount_management_ui=debug"
  ]
  ++ optional (ui.tlsCertPath != "") "SURMOUNT_TLS_CERT=${ui.tlsCertPath}"
  ++ optional (ui.tlsKeyPath != "") "SURMOUNT_TLS_KEY=${ui.tlsKeyPath}"
  ++ optional ui.allowCleartextHttpsEscape "SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1"
  ++ optional redirectListenActive "SURMOUNT_HTTP_REDIRECT_LISTEN=${ui.httpRedirectListen}"
  ++ optional localCleartextActive "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=${effectiveLocalCleartext}"
  ++ optional (ui.onionUrl != "") "SURMOUNT_ONION_URL=${ui.onionUrl}"
  ++ optional (ui.onionHostnameFile != "") "SURMOUNT_ONION_HOSTNAME_FILE=${ui.onionHostnameFile}"
  ++ [
    "SURMOUNT_AUTH_MODE=${ui.authMode}"
    "SURMOUNT_SESSION_TTL_SECS=${toString ui.sessionTtlSecs}"
    "SURMOUNT_NIP98_MAX_SKEW_SECS=${toString ui.nip98MaxSkewSecs}"
  ]
  ++ optional (ui.nostrAllowlist != "") "SURMOUNT_NOSTR_ALLOWLIST=${ui.nostrAllowlist}"
  ++ optional (ui.nostrAllowlistFile != "") "SURMOUNT_NOSTR_ALLOWLIST_FILE=${ui.nostrAllowlistFile}"
  ++ optional (ui.publicBaseUrl != "") "SURMOUNT_PUBLIC_BASE_URL=${ui.publicBaseUrl}"
  # Lab-only inline secret; prefer sessionSecretPath + EnvironmentFile.
  ++ optional (ui.sessionSecretEnv != "") "SURMOUNT_SESSION_SECRET=${ui.sessionSecretEnv}"
  # Directory: default unavailable (honest empty). mock/stalwart only when set.
  # Never default-on live or mock (no fake production accounts).
  ++ optional (ui.directory != "unavailable") "SURMOUNT_DIRECTORY=${ui.directory}"
  ++ optional (
    ui.stalwartTokenEnv != "" && ui.allowLabInlineStalwartToken
  ) "SURMOUNT_STALWART_TOKEN=${ui.stalwartTokenEnv}"
  ++ optional (ui.stalwartTokenPath != "") "SURMOUNT_STALWART_TOKEN_FILE=${ui.stalwartTokenPath}"
  ++ optional ui.allowDirectoryUnauthenticated "SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1"
  ++ lib.optionals ac.enable (
    [
      "SURMOUNT_BAN_ENFORCEMENT=${ac.enforcement}"
      "SURMOUNT_BAN_BACKEND=${ac.backend}"
    ]
    ++ optional (banWhitelistCsv != "") "SURMOUNT_BAN_WHITELIST=${banWhitelistCsv}"
    ++ optional (banStatePath != "") "SURMOUNT_BAN_STATE_PATH=${banStatePath}"
    ++ optional ac.nftExec "SURMOUNT_BAN_NFT_EXEC=1"
    # nftBin for direct exec; helper unit gets its own env (not required on UI
    # when only UDS path is used, but Enforce+helper validate still needs it).
    ++ optional ((ac.nftExec || ac.nftHelper) && ac.nftBin != "") "SURMOUNT_BAN_NFT_BIN=${ac.nftBin}"
    # Product elevation: UDS to socket-activated oneshot (not child setcap).
    # UI keeps NoNewPrivileges; CAP_NET_ADMIN is only on the helper unit.
    ++ optional ac.nftHelper "SURMOUNT_BAN_NFT_HELPER_SOCK=/run/surmount/nft-ban-helper.sock"
  );

  tlsReadPaths = lib.filter (p: p != null && p != "") [
    ui.tlsCertPath
    ui.tlsKeyPath
    ui.onionHostnameFile
    ui.sessionSecretPath
    ui.stalwartTokenPath
  ];

  # EnvironmentFile expects KEY=value lines (e.g. SURMOUNT_SESSION_SECRET=...).
  # Host-only deploy secret; never in git. Empty path skips.
  sessionSecretEnvFiles = lib.optional (ui.sessionSecretPath != "") ui.sessionSecretPath;

  # Real TLS path (not cleartext escape): gate unit start on PEM files so a
  # missing deploy secret yields inactive (dead), not Restart=on-failure thrash.
  httpsNeedsHostPems =
    ui.listenMode == "https"
    && !ui.allowCleartextHttpsEscape
    && ui.tlsCertPath != ""
    && ui.tlsKeyPath != "";

  # Live directory needs a host token file when path is set (lab may use
  # allowed inline env only). Missing path would fail-closed at process start
  # and Restart=on-failure thrash; ConditionPathExists keeps the unit inactive.
  # Path gates existence only; file must still be non-empty raw token, owner-only
  # mode (e.g. 0600), readable by surmount-ui (binary enforces mode + content).
  directoryNeedsTokenFile =
    ui.directory == "stalwart"
    && ui.stalwartTokenPath != ""
    && !(ui.stalwartTokenEnv != "" && ui.allowLabInlineStalwartToken);

  # Eval fail-closed: stalwart mode without any allowed token source is misconfig.
  directoryStalwartTokenOk =
    ui.directory != "stalwart"
    || ui.stalwartTokenPath != ""
    || (ui.stalwartTokenEnv != "" && ui.allowLabInlineStalwartToken);

  # Live directory + open auth is a footgun; require nostr or lab escape.
  directoryStalwartAuthOk =
    ui.directory != "stalwart" || ui.authMode == "nostr" || ui.allowDirectoryUnauthenticated;

  # Inline token via Nix Environment= only with explicit lab flag.
  directoryLabInlineTokenOk = ui.stalwartTokenEnv == "" || ui.allowLabInlineStalwartToken;

  isLoopbackListenAddr = localCleartext.isLoopbackListenAddr ui.listenAddress;

  # Dual-run nginx owns public :80/:443. Fail closed if UI also claims public
  # HTTPS (:443 or non-loopback https) while web.enable is on.
  dualRunPublicHttpsConflict =
    cfg.web.enable && ui.listenMode == "https" && (ui.port == 443 || !isLoopbackListenAddr);

  # Dual-run nginx owns :80 redirect/ACME. Product redirect bind must not share.
  dualRunRedirectConflict = cfg.web.enable && ui.redirectHttpToHttps;

  # Product redirect is an HTTPS upgrade path; plain http primary is a footgun
  # (Location points at https://… with no TLS terminator on that port).
  redirectRequiresHttps = ui.redirectHttpToHttps && ui.listenMode != "https";

  # Trailing :port from host:port or [v6]:port (null if shape is wrong).
  redirectListenPort = localCleartext.listenPort ui.httpRedirectListen;

  # Non-empty listen must look like host:port so binary parse cannot surprise.
  redirectListenShapeOk = !redirectListenActive || redirectListenPort != null;

  localCleartextPort = localCleartext.listenPort effectiveLocalCleartext;
  localCleartextShapeOk = !localCleartextActive || localCleartextPort != null;
  localCleartextLoopbackOk =
    !localCleartextActive || localCleartext.isLoopbackCleartextTarget effectiveLocalCleartext;

  primaryListen = "${ui.listenAddress}:${toString ui.port}";
  localCleartextDiffersPrimary = !localCleartextActive || effectiveLocalCleartext != primaryListen;
  localCleartextDiffersRedirect =
    !localCleartextActive || !redirectListenActive || effectiveLocalCleartext != ui.httpRedirectListen;
  # Linux: 0.0.0.0:P and 127.0.0.1:P cannot both bind. Port-level collision
  # must fail closed even when host strings differ.
  localCleartextPortCollision =
    localCleartextActive
    && localCleartext.localCleartextPortCollides {
      inherit ui;
      localListen = effectiveLocalCleartext;
    };

  # Non-root bind on privileged ports only (same pattern as Stalwart).
  needsNetBindService =
    ui.port < 1024
    || (redirectListenActive && redirectListenPort != null && redirectListenPort < 1024)
    || (localCleartextActive && localCleartextPort != null && localCleartextPort < 1024);

  serviceConfigBase = {
    Type = "simple";
    User = "surmount-ui";
    Group = "surmount-ui";
    ExecStart = "${pkg}/bin/surmount-management-ui";
    Restart = "on-failure";
    RestartSec = "5s";
    Environment = envList;
    NoNewPrivileges = true;
    PrivateTmp = true;
    ProtectSystem = "strict";
    ProtectHome = true;
    ProtectKernelTunables = true;
    ProtectKernelModules = true;
    ProtectControlGroups = true;
    RestrictAddressFamilies = [
      "AF_INET"
      "AF_INET6"
      "AF_UNIX"
    ];
    RestrictNamespaces = true;
    LockPersonality = true;
    MemoryDenyWriteExecute = true;
    RestrictRealtime = true;
    SystemCallArchitectures = "native";
    ReadWritePaths = [ cfg.stateDir ];
  };

  serviceConfig =
    serviceConfigBase
    // lib.optionalAttrs (tlsReadPaths != [ ]) {
      # Host deploy-secret PEMs / session EnvironmentFile (operator-placed; never in git).
      ReadOnlyPaths = tlsReadPaths;
    }
    // lib.optionalAttrs (sessionSecretEnvFiles != [ ]) {
      EnvironmentFile = sessionSecretEnvFiles;
    }
    // lib.optionalAttrs needsNetBindService {
      AmbientCapabilities = [ "CAP_NET_BIND_SERVICE" ];
      CapabilityBoundingSet = [ "CAP_NET_BIND_SERVICE" ];
    };

  unitConditionPaths =
    (lib.optionals httpsNeedsHostPems [
      ui.tlsCertPath
      ui.tlsKeyPath
    ])
    ++ (lib.optionals directoryNeedsTokenFile [ ui.stalwartTokenPath ]);

  unitConfig = {
    # Cap restart storms when binary fail-closes (bad PEMs, wrong key mode,
    # redirect bind refuse, incomplete directory token). Mirrors
    # surmount-arti-hidden-service; missing PEMs/token file also use
    # ConditionPathExists below so multi-user is not wedged forever.
    StartLimitIntervalSec = 300;
    StartLimitBurst = 5;
  }
  // lib.optionalAttrs (unitConditionPaths != [ ]) {
    ConditionPathExists = unitConditionPaths;
  };
in
{
  config = mkIf (cfg.enable && ui.enable) {
    assertions = [
      {
        assertion = pkg != null;
        message = ''
          surmount.managementUi is enabled but no package is available.
          Ensure the flake overlay provides pkgs.surmount-management-ui
          or set surmount.managementUi.package explicitly.
        '';
      }
      {
        assertion = ui.listenMode != "https" || (ui.tlsCertPath != "" && ui.tlsKeyPath != "");
        message = ''
          surmount.managementUi.listenMode is "https" but tlsCertPath /
          tlsKeyPath are empty. Point them at host deploy-secret PEMs
          under e.g. /run/surmount-secrets/tls/ (never in git).
        '';
      }
      {
        # Escape is emergency cleartext only; never required for real HTTPS.
        # When escape is on, PEMs are still required by the binary config parse
        # for listenMode=https (paths must be set even if not used for TLS).
        assertion = !ui.allowCleartextHttpsEscape || ui.listenMode == "https";
        message = ''
          surmount.managementUi.allowCleartextHttpsEscape is true but
          listenMode is not "https". The escape only applies to https mode
          (sets SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE). Turn the escape off
          or set listenMode = "https" with tlsCertPath/tlsKeyPath.
        '';
      }
      {
        assertion =
          hostPaths.optionalStrictHostPath ui.tlsCertPath && hostPaths.optionalStrictHostPath ui.tlsKeyPath;
        message = ''
          surmount.managementUi tlsCertPath/tlsKeyPath must be empty or
          strict absolute host paths (/[A-Za-z0-9._/-]+, no metacharacters).
        '';
      }
      {
        assertion = !dualRunPublicHttpsConflict;
        message = ''
          surmount.web.enable is true (transitional nginx owns public :80/:443)
          but managementUi.listenMode is "https" on a public-facing bind
          (${ui.listenAddress}:${toString ui.port}). Dual-run escape keeps the
          UI on listenMode=http loopback behind nginx. Either set
          web.enable = false for Axum-first public HTTPS, or use
          listenMode = "http" on 127.0.0.1 (not port 443).
        '';
      }
      {
        assertion = !dualRunRedirectConflict;
        message = ''
          surmount.web.enable is true (transitional nginx owns public :80
          redirect/ACME) but managementUi.redirectHttpToHttps is true. Do not
          dual-bind product :80 with nginx. Either set web.enable = false and
          use the Axum redirect-only listener, or keep redirectHttpToHttps =
          false while nginx is the public edge.
        '';
      }
      {
        assertion = !redirectRequiresHttps;
        message = ''
          surmount.managementUi.redirectHttpToHttps is true but listenMode is
          not "https". The redirect listener upgrades to HTTPS using the primary
          listen port; plain http primary would emit https:// Location with no
          TLS terminator. Set listenMode = "https" with tlsCertPath/tlsKeyPath,
          or turn redirectHttpToHttps off.
        '';
      }
      {
        assertion = redirectListenShapeOk;
        message = ''
          surmount.managementUi.httpRedirectListen must be empty or a host:port
          socket address (e.g. 0.0.0.0:80 or [::]:80) when redirectHttpToHttps
          is true. Got: ${ui.httpRedirectListen}
        '';
      }
      {
        assertion = localCleartextShapeOk;
        message = ''
          surmount.managementUi.localCleartextListen must be null/empty or a
          host:port socket address (e.g. 127.0.0.1:8090). Got: ${toString effectiveLocalCleartext}
        '';
      }
      {
        assertion = localCleartextLoopbackOk;
        message = ''
          surmount.managementUi local cleartext API must bind loopback only
          (127.0.0.1 or [::1] only; not localhost hostnames). Got: ${toString effectiveLocalCleartext}.
          Do not expose a public cleartext management API.
        '';
      }
      {
        assertion = localCleartextDiffersPrimary;
        message = ''
          surmount.managementUi local cleartext listen (${toString effectiveLocalCleartext})
          must differ from the primary listen (${primaryListen}).
        '';
      }
      {
        assertion = localCleartextDiffersRedirect;
        message = ''
          surmount.managementUi local cleartext listen must differ from the
          redirect-only listen (${ui.httpRedirectListen}). Redirect :80 stays
          redirect-only (no cleartext API / no ACME product invent on that port).
        '';
      }
      {
        assertion = !localCleartextPortCollision;
        message = ''
          surmount.managementUi local cleartext port collides with primary
          (${toString ui.port}) or redirect listen port. Linux cannot bind
          0.0.0.0:P and 127.0.0.1:P together. Auto-derive picks a free loopback
          port; if you set localCleartextListen explicitly, choose a port
          different from managementUi.port and httpRedirectListen.
          Got local=${toString effectiveLocalCleartext}.
        '';
      }
      {
        assertion = directoryStalwartTokenOk;
        message = ''
          surmount.managementUi.directory is "stalwart" but no allowed token
          source is set. Prefer stalwartTokenPath (host file, non-empty raw
          token, mode 0600, readable by surmount-ui). Lab-only:
          stalwartTokenEnv + allowLabInlineStalwartToken = true. Leave
          directory = "unavailable" (default) for honest empty accounts.
        '';
      }
      {
        assertion = directoryStalwartAuthOk;
        message = ''
          surmount.managementUi.directory is "stalwart" but authMode is not
          "nostr". Live principal list must not be open on an unauthenticated
          bind. Set authMode = "nostr" (with session secret + allowlist), or
          lab-only allowDirectoryUnauthenticated = true.
        '';
      }
      {
        assertion = directoryLabInlineTokenOk;
        message = ''
          surmount.managementUi.stalwartTokenEnv is set but
          allowLabInlineStalwartToken is false. Inline tokens go into unit
          Environment= and can land in the Nix store. Production: use
          stalwartTokenPath only. Lab: set allowLabInlineStalwartToken = true.
        '';
      }
      {
        assertion = hostPaths.optionalStrictHostPath ui.stalwartTokenPath;
        message = ''
          surmount.managementUi.stalwartTokenPath must be empty or a strict
          absolute host path (no .. segments; charset limited). Host-only
          deploy secret; never in git. File must be non-empty raw token
          (not comments only), regular file, mode not group/world readable.
        '';
      }
    ];

    warnings = lib.optional (ui.stalwartTokenEnv != "" && ui.allowLabInlineStalwartToken) ''
      surmount.managementUi.stalwartTokenEnv is set (lab inline Bearer). Prefer
      stalwartTokenPath for production so the secret stays a host file and out
      of the Nix store / unit Environment=.
    '';

    users.groups.surmount-ui = { };
    users.users.surmount-ui = {
      isSystemUser = true;
      group = "surmount-ui";
      description = "Surmount management UI";
    };

    systemd.services.surmount-management-ui = {
      description = "Surmount management UI (Axum edge foundation)";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ] ++ lib.optionals ac.nftHelper [ "surmount-nft-ban-helper.socket" ];
      wants = lib.optionals ac.nftHelper [ "surmount-nft-ban-helper.socket" ];
      unitConfig = unitConfig;
      serviceConfig = serviceConfig;
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir}/ui 0750 surmount-ui surmount-ui - -"
    ];
  };
}
