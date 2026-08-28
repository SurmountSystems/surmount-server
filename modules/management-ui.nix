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
  # Proven DS3018xs static HTML (apex + www). Host-local roots; populate
  # with just sync-static-sites-from-ds3018xs. Do not vendor media into git.
  staticSiteRoot = slug: "/var/lib/surmount/static-sites/${slug}";
  provenStaticVhosts =
    let
      pair = hostname: slug: {
        ${hostname} = {
          root = staticSiteRoot slug;
        };
        "www.${hostname}" = {
          root = staticSiteRoot slug;
        };
      };
    in
    (pair "cryptoquick.com" "cryptoquick")
    // (pair "baxterartworks.com" "baxterartworks")
    // (pair "btcfur.com" "btcfur")
    // (pair "exophiles.org" "exophiles")
    // (pair "iantuckerstudios.com" "iantuckerstudios")
    // (pair "nostrfurs.com" "nostrfurs")
    // (pair "yiffa.app" "yiffa");
  staticVhostHostOk =
    h:
    builtins.match "[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?" h != null
    && !(lib.hasInfix ".." h)
    && lib.hasInfix "." h;
  staticVhostsJsonMap = lib.mapAttrs (_n: v: v.root) ui.staticVhosts;
  staticVhostReadPaths = lib.unique (map (v: v.root) (lib.attrValues ui.staticVhosts));
  staticVhostsFile =
    if ui.staticVhosts == { } then
      ""
    else
      "${pkgs.writeText "surmount-static-vhosts.json" (builtins.toJSON staticVhostsJsonMap)}";
  hs = cfg.artiHiddenService;
  ac = cfg.accessControl;
  vw = cfg.vaultwarden;

  # Console link to domain C vault:
  # 1) explicit managementUi.vaultwardenUrl wins
  # 2) else when path proxy on: public https://{servicesHostname}{prefix} shape
  # 3) else when VW module enabled: honest private loopback from rocket listen
  # Never invent without enable/proxy. Never ADMIN_TOKEN.
  effectiveVaultwardenProxyPrefix =
    let
      p = ui.vaultwardenProxyPrefix;
    in
    if lib.hasPrefix "/" p then (lib.removeSuffix "/" p) else "/${lib.removeSuffix "/" p}";

  effectiveVaultwardenProxyUpstream =
    if ui.vaultwardenProxyUpstream != "" then
      ui.vaultwardenProxyUpstream
    else if vw.enable then
      "http://${vw.rocketAddress}:${toString vw.rocketPort}"
    else
      "http://127.0.0.1:8222";

  effectiveVaultwardenUrl =
    if ui.vaultwardenUrl != "" then
      ui.vaultwardenUrl
    else if ui.vaultwardenProxyEnable then
      "https://${cfg.servicesHostname}${effectiveVaultwardenProxyPrefix}"
    else if vw.enable then
      "http://${vw.rocketAddress}:${toString vw.rocketPort}"
    else
      "";

  # Onion hostname file for console surface: explicit managementUi.onionHostnameFile
  # wins; else when artiHiddenService is enabled, derive the conventional host path
  # under onionServiceStateDir (operator places hostname after Arti publishes).
  # Never invent a live .onion. Empty with Arti off = not provisioned in UI.
  effectiveOnionHostnameFile =
    if ui.onionHostnameFile != "" then
      ui.onionHostnameFile
    else if hs.enable then
      "${hs.onionServiceStateDir}/hostname"
    else
      "";

  # Environment= safe URL: http(s) only; limited charset (no whitespace,
  # quotes, $, backticks, semicolons, newlines). Matches management-ui
  # normalize_vaultwarden_url refuse path for non-http schemes.
  # Char class: put - last; omit raw ] so the class cannot close early.
  vaultwardenUrlShapeOk =
    let
      s = effectiveVaultwardenUrl;
    in
    s == ""
    || (
      (lib.hasPrefix "http://" s || lib.hasPrefix "https://" s)
      && builtins.match "https?://[A-Za-z0-9._~:/%@&=+?#\\[\\-]+" s != null
    );

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
    # MTA-STS skeleton (default off). Policy path /.well-known/mta-sts.txt.
    "SURMOUNT_MTA_STS_MODE=${ui.mtaStsMode}"
    "SURMOUNT_MTA_STS_MAX_AGE=${toString ui.mtaStsMaxAge}"
    "RUST_LOG=info,surmount_management_ui=debug"
  ]
  ++ optional (
    ui.apexPublicRoot != null && ui.apexPublicRoot != ""
  ) "SURMOUNT_APEX_PUBLIC_ROOT=${ui.apexPublicRoot}"
  ++ optional (staticVhostsFile != "") "SURMOUNT_STATIC_VHOSTS_FILE=${staticVhostsFile}"
  ++ optional (ui.tlsCertPath != "") "SURMOUNT_TLS_CERT=${ui.tlsCertPath}"
  ++ optional (ui.tlsKeyPath != "") "SURMOUNT_TLS_KEY=${ui.tlsKeyPath}"
  ++ optional ui.allowCleartextHttpsEscape "SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1"
  # In-process ACME (default off). Env only when enable so binary stays quiet.
  ++ lib.optionals ui.acme.enable (
    [
      "SURMOUNT_ACME_ENABLE=1"
      "SURMOUNT_ACME_CHALLENGE=${ui.acme.challenge}"
      "SURMOUNT_ACME_DNS_PROVIDER=${ui.acme.dnsProvider}"
      "SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY=${toString ui.acme.renewDaysBeforeExpiry}"
    ]
    ++ optional (ui.acme.directory != "") "SURMOUNT_ACME_DIRECTORY=${ui.acme.directory}"
    ++ optional (ui.acme.email != "") "SURMOUNT_ACME_EMAIL=${ui.acme.email}"
    ++ optional (
      ui.acme.domains != [ ]
    ) "SURMOUNT_ACME_DOMAINS=${lib.concatStringsSep "," ui.acme.domains}"
    ++ optional (
      ui.acme.accountCredentialsPath != ""
    ) "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH=${ui.acme.accountCredentialsPath}"
    ++ optional (ui.acme.dnsHookPath != "") "SURMOUNT_ACME_DNS_HOOK=${ui.acme.dnsHookPath}"
    ++ optional (ui.acme.dnsProvider == "external-hook") (
      "SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS=${toString ui.acme.dnsHookTimeoutSecs}"
    )
  )
  ++ optional redirectListenActive "SURMOUNT_HTTP_REDIRECT_LISTEN=${ui.httpRedirectListen}"
  ++ optional localCleartextActive "SURMOUNT_LOCAL_CLEARTEXT_LISTEN=${effectiveLocalCleartext}"
  ++ optional (ui.onionUrl != "") "SURMOUNT_ONION_URL=${ui.onionUrl}"
  ++ optional (
    effectiveOnionHostnameFile != ""
  ) "SURMOUNT_ONION_HOSTNAME_FILE=${effectiveOnionHostnameFile}"
  # When Arti HS module is on, expose state dir so the binary can walk for
  # hostname material (Arti layout may nest under keystore). Never invent.
  ++ optional hs.enable "SURMOUNT_ONION_HS_STATE_DIR=${hs.onionServiceStateDir}"
  # Domain C Vaultwarden console link (operator-published, proxy public URL, or loopback).
  # Never ADMIN_TOKEN. Empty = residual not configured in UI.
  ++ optional (effectiveVaultwardenUrl != "") "SURMOUNT_VAULTWARDEN_URL=${effectiveVaultwardenUrl}"
  # Axum path reverse-proxy to loopback Rocket (default off). No nginx.
  ++ lib.optionals ui.vaultwardenProxyEnable [
    "SURMOUNT_VAULTWARDEN_PROXY=1"
    "SURMOUNT_VAULTWARDEN_PROXY_PREFIX=${effectiveVaultwardenProxyPrefix}"
    "SURMOUNT_VAULTWARDEN_PROXY_UPSTREAM=${effectiveVaultwardenProxyUpstream}"
  ]
  ++ [
    "SURMOUNT_AUTH_MODE=${ui.authMode}"
    "SURMOUNT_SESSION_TTL_SECS=${toString ui.sessionTtlSecs}"
    "SURMOUNT_NIP98_MAX_SKEW_SECS=${toString ui.nip98MaxSkewSecs}"
  ]
  ++ optional (ui.nostrAllowlist != "") "SURMOUNT_NOSTR_ALLOWLIST=${ui.nostrAllowlist}"
  ++ optional (ui.nostrAllowlistFile != "") "SURMOUNT_NOSTR_ALLOWLIST_FILE=${ui.nostrAllowlistFile}"
  ++ optional (ui.publicBaseUrl != "") "SURMOUNT_PUBLIC_BASE_URL=${ui.publicBaseUrl}"
  ++ [
    "SURMOUNT_CONSOLE_ACCOUNTS=${
      if ui.consoleAccountsFile != "" then
        ui.consoleAccountsFile
      else
        "${cfg.stateDir}/console/accounts.json"
    }"
    "SURMOUNT_NWC_STORE=${
      if ui.nwcStoreFile != "" then ui.nwcStoreFile else "${cfg.secrets.durableMaterialDir}/ui/nwc.json"
    }"
  ]
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
  ++ optional ui.allowPublicAuthOff "SURMOUNT_ALLOW_PUBLIC_AUTH_OFF=1"
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

  # Non-TLS host secret paths stay read-only. TLS PEMs are read-only when
  # ACME is off; when ACME is on the binary may write cert/key + account JSON.
  # ProtectSystem=strict: create-on-issue needs **parent directories** writable
  # (bare file paths do not reliably allow creating missing PEMs).
  tlsPemPaths = lib.filter (p: p != null && p != "") [
    ui.tlsCertPath
    ui.tlsKeyPath
  ];
  acmeParentDirs =
    let
      parents = map builtins.dirOf (
        lib.filter (p: p != null && p != "") [
          ui.tlsCertPath
          ui.tlsKeyPath
          ui.acme.accountCredentialsPath
        ]
      );
      # Drop empty / root; unique so cert+key in same dir appear once.
      usable = lib.filter (p: p != null && p != "" && p != "/") parents;
    in
    lib.unique usable;
  acmeWritePaths = lib.optionals ui.acme.enable acmeParentDirs;
  # Operator DNS-01 hook is read/exec only (usually /run or /nix/store); not
  # ReadWritePaths. When empty, omitted.
  acmeHookReadPaths = lib.optionals (ui.acme.enable && ui.acme.dnsHookPath != "") [
    ui.acme.dnsHookPath
  ];
  tlsReadPaths = lib.filter (p: p != null && p != "") (
    (lib.optionals (!ui.acme.enable) tlsPemPaths)
    ++ acmeHookReadPaths
    ++ [
      effectiveOnionHostnameFile
      ui.sessionSecretPath
      ui.stalwartTokenPath
      ui.nostrAllowlistFile
    ]
    ++ lib.optionals (ui.apexPublicRoot != null && ui.apexPublicRoot != "") [
      ui.apexPublicRoot
    ]
    ++ staticVhostReadPaths
    # Allow reading hostname material under HS state dir when Arti is enabled
    # (nested hostname files; missing path is ignored by systemd ReadOnlyPaths).
    ++ lib.optionals hs.enable [ hs.onionServiceStateDir ]
  );

  # EnvironmentFile expects KEY=value lines (e.g. SURMOUNT_SESSION_SECRET=...).
  # Host-only deploy secret; never in git. Empty path skips.
  sessionSecretEnvFiles = lib.optional (ui.sessionSecretPath != "") ui.sessionSecretPath;

  # Real TLS path (not cleartext escape): gate unit start on PEM files so a
  # missing deploy secret yields inactive (dead), not Restart=on-failure thrash.
  # When in-process ACME is enabled, PEMs may be missing at first start (binary
  # issues them); skip ConditionPathExists so the unit can run issuance.
  httpsNeedsHostPems =
    ui.listenMode == "https"
    && !ui.allowCleartextHttpsEscape
    && !ui.acme.enable
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

  # Public primary listen + authMode=off is an open-console footgun.
  # Loopback primary may keep auth-off for local/dev. Lab escape:
  # allowPublicAuthOff (never production default). Mirrors binary
  # require_auth_when_public_edge / SURMOUNT_ALLOW_PUBLIC_AUTH_OFF.
  publicPrimaryAuthOk = ui.authMode != "off" || isLoopbackListenAddr || ui.allowPublicAuthOff;

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
    SyslogIdentifier = "surmount-management-ui";
    StandardOutput = "journal";
    StandardError = "journal";
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
    ReadWritePaths = [ cfg.stateDir ] ++ acmeWritePaths;
  };

  serviceConfig =
    serviceConfigBase
    // lib.optionalAttrs (tlsReadPaths != [ ]) {
      # Host deploy-secret PEMs / session EnvironmentFile (operator-placed; never in git).
      # When ACME is on, PEMs are under ReadWritePaths instead (issuance write).
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
    ++ (lib.optionals directoryNeedsTokenFile [ ui.stalwartTokenPath ])
    ++ (lib.optionals (ui.authMode == "nostr" && ui.nostrAllowlistFile != "") [
      ui.nostrAllowlistFile
    ])
    ++ (lib.optionals (ui.authMode == "nostr" && ui.sessionSecretPath != "") [
      ui.sessionSecretPath
    ]);

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

  # Packaged DNS-01 Namecheap helper (**code**, not secrets). When ACME uses
  # external-hook and the operator did not set dnsHookPath, default to the
  # Nix store path so the unit execs a regular file with safe modes.
  # Content SoT: nix run .#acme-dns-hook-namecheap-bin. Credentials still
  # only: laptop -> secrets-install-host -> durable Domain B namecheap.env
  # (default /var/lib/surmount/secrets/acme/namecheap.env; /run still allowed).
  packagedDnsHookPath =
    if pkgs ? acme-dns-hook-namecheap then
      "${pkgs.acme-dns-hook-namecheap}/bin/acme-dns-hook-namecheap"
    else
      "";

  # Packaged SurmountSystems/site static tree. When the overlay provides
  # pkgs.surmount-public-site, default apex/www to that store path so a
  # host generation includes the site without a mutable /var/lib copy.
  # Operator may still set apexPublicRoot to a host directory.
  packagedPublicSite = if pkgs ? surmount-public-site then "${pkgs.surmount-public-site}" else "";
in
{
  config = mkIf (cfg.enable && ui.enable) {
    # Prefer store-path hook when external-hook is on (overridable).
    surmount.managementUi.acme.dnsHookPath = lib.mkIf (
      ui.acme.enable && ui.acme.dnsProvider == "external-hook" && packagedDnsHookPath != ""
    ) (lib.mkDefault packagedDnsHookPath);
    surmount.managementUi.apexPublicRoot = lib.mkIf (packagedPublicSite != "") (
      lib.mkDefault packagedPublicSite
    );
    surmount.managementUi.staticVhosts = lib.mkDefault provenStaticVhosts;
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
        assertion = lib.all staticVhostHostOk (lib.attrNames ui.staticVhosts);
        message = ''
          surmount.managementUi.staticVhosts keys must be DNS hostnames
          (letter/digit start, a dot, no ..). Include www aliases as their
          own keys pointing at the same root.
        '';
      }
      {
        assertion = lib.all hostPaths.strictHostPath staticVhostReadPaths;
        message = ''
          surmount.managementUi.staticVhosts.*.root must be strict absolute
          host paths (/[A-Za-z0-9._/-]+, no metacharacters).
        '';
      }
      {
        # In-process ACME default-off; when enable, fail closed at eval on incomplete knobs.
        assertion =
          !ui.acme.enable
          || (
            ui.acme.directory != ""
            && ui.acme.email != ""
            && ui.acme.domains != [ ]
            && ui.acme.accountCredentialsPath != ""
            && ui.listenMode == "https"
            && ui.tlsCertPath != ""
            && ui.tlsKeyPath != ""
          );
        message = ''
          surmount.managementUi.acme.enable is true but incomplete: require
          non-empty acme.directory, acme.email, acme.domains, acme.accountCredentialsPath,
          listenMode=https, and tlsCertPath/tlsKeyPath (write targets for PEMs).
          Default is acme.enable=false. Prefer staging directory first. Account
          credentials and PEMs stay on the host (never in git). HTTP-01 on
          product :80 is parked; only dns-01 is wired.
        '';
      }
      {
        assertion = !ui.acme.enable || hostPaths.optionalStrictHostPath ui.acme.accountCredentialsPath;
        message = ''
          surmount.managementUi.acme.accountCredentialsPath must be a strict
          absolute host path when acme.enable is true (never in git).
        '';
      }
      {
        assertion =
          !ui.acme.enable
          || ui.acme.dnsProvider != "external-hook"
          || (ui.acme.dnsHookPath != "" && hostPaths.optionalStrictHostPath ui.acme.dnsHookPath);
        message = ''
          surmount.managementUi.acme.dnsProvider is "external-hook" but
          acme.dnsHookPath is empty or not a strict absolute host path.
          Point dnsHookPath at an operator-owned executable that implements
          DNS-01 set/clear (and optional wait). Never put secrets in git.
        '';
      }
      {
        # Mirror Rust fail-closed: lab mock must not target production LE.
        assertion =
          !ui.acme.enable
          || ui.acme.dnsProvider != "mock"
          || !(
            ui.acme.directory == "https://acme-v02.api.letsencrypt.org/directory"
            || lib.hasPrefix "https://acme-v02.api.letsencrypt.org/" ui.acme.directory
          );
        message = ''
          surmount.managementUi.acme.dnsProvider = "mock" cannot use the
          production Let's Encrypt directory (lab/self-signed only). Use
          staging (https://acme-staging-v02.api.letsencrypt.org/directory)
          or a non-production directory, or switch dnsProvider to external-hook.
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
        assertion = vaultwardenUrlShapeOk;
        message = ''
          surmount.managementUi.vaultwardenUrl (or derived Vaultwarden loopback
          URL) must be empty or http(s):// with Environment=-safe charset
          (no whitespace, quotes, $, backticks, semicolons). Got:
          ${effectiveVaultwardenUrl}
          Scheme must be http or https only (no javascript:/data:/file:).
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
        assertion = publicPrimaryAuthOk;
        message = ''
          surmount.managementUi.authMode is "off" but primary listen
          (${ui.listenAddress}:${toString ui.port}) is not loopback. Open
          console is not public-safe. Set authMode = "nostr" (session secret +
          allowlist), bind listenAddress to loopback (127.0.0.1 / ::1), or
          lab-only allowPublicAuthOff = true (never production). Binary also
          refuse-starts the same coupling.
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
    users.groups.surmount-tls = { };
    users.users.surmount-ui = {
      isSystemUser = true;
      group = "surmount-ui";
      extraGroups = [ "surmount-tls" ];
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

    # State dir always (0755 so surmount-ui can traverse to durable Domain B
    # under stateDir/secrets; H2b). Durable Domain B leaves for runtime secrets
    # + H-PEM tls/ so UI can read/write after install. When ACME is on, also
    # ensure parents of configured PEM/account paths under deployMaterialDir
    # (ephemeral /run) and/or durableMaterialDir. Owner surmount-ui so the unit
    # can write without pre-placed CA PEMs. Does not place cert.pem / key.pem /
    # account.json contents (never git).
    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0755 root root - -"
      "d ${cfg.stateDir}/ui 0750 surmount-ui surmount-ui - -"
      "d ${cfg.stateDir}/console 0750 surmount-ui surmount-ui - -"
      # Durable Domain B leaves (S8 + H-PEM): session/token/namecheap + PEMs
      "d ${cfg.secrets.durableMaterialDir}/ui 0750 surmount-ui surmount-ui - -"
      "d ${cfg.secrets.durableMaterialDir}/acme 0750 surmount-ui surmount-ui - -"
      "d ${cfg.secrets.durableMaterialDir}/tls 0750 surmount-ui surmount-tls - -"
      "z ${cfg.secrets.durableMaterialDir}/tls/cert.pem 0640 surmount-ui surmount-tls -"
      "z ${cfg.secrets.durableMaterialDir}/tls/key.pem 0600 surmount-ui surmount-tls -"
    ]
    ++ lib.optionals (staticVhostReadPaths != [ ]) (
      [ "d ${cfg.stateDir}/static-sites 0755 surmount-ui surmount-ui - -" ]
      ++ map (p: "d ${p} 0755 surmount-ui surmount-ui - -") (
        lib.filter (p: lib.hasPrefix "${cfg.stateDir}/static-sites/" p) staticVhostReadPaths
      )
    )
    ++ lib.optionals ui.acme.enable (
      let
        # Material roots: traversable 0755; leaf parents: UI-owned 0750.
        ephemeralRoot = cfg.secrets.deployMaterialDir;
        durableRoot = cfg.secrets.durableMaterialDir;
        underRoot = root: p: p == root || lib.hasPrefix (root + "/") p;
        underEphemeral = underRoot ephemeralRoot;
        underDurable = underRoot durableRoot;
        parentRules = map (p: "d ${p} 0750 surmount-ui surmount-ui - -") (
          lib.filter (p: underEphemeral p || underDurable p) acmeParentDirs
        );
        rootRules =
          (lib.optional (builtins.any underEphemeral acmeParentDirs) "d ${ephemeralRoot} 0755 root root - -")
          ++ (lib.optional (builtins.any underDurable acmeParentDirs) "d ${durableRoot} 0755 root root - -");
      in
      rootRules ++ parentRules
    );
  };
}
