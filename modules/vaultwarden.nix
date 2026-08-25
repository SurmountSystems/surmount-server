# Surmount domain C: Vaultwarden human / org password vault.
#
# Wraps stock nixpkgs services.vaultwarden with private single-VPS defaults:
# SQLite, loopback Rocket, signups disabled, admin token from host
# EnvironmentFile only (never inline secret in the public tree).
#
# Not: deploy-secret activation feed for other units; Bitwarden Secrets
# Manager API (VW is password-manager API only; Q-SEC-SM-* stay open);
# nginx product edge; public open registration.
#
# Data path (stateVersion >= 24.11): /var/lib/vaultwarden (stock StateDirectory).
# Admin token example path (comments only): /var/lib/surmount/secrets/vaultwarden/admin.env
# Install material with nix run .#secrets-install-host (domain B). See docs/SECRETS.md.

{
  config,
  lib,
  ...
}:
let
  cfg = config.surmount;
  vw = cfg.vaultwarden;
  hostPaths = import ./lib/host-paths.nix { inherit lib; };
  inherit (lib) mkIf;

  vwOn = cfg.enable && vw.enable;

  # Stock data dir for stateVersion >= 24.11 (host channel 26.05).
  dataDir = "/var/lib/vaultwarden";

  # Stock nixpkgs writes services.vaultwarden.config to a world-readable store
  # EnvironmentFile (vaultwarden.env). Secret-bearing keys must never land there;
  # use host environmentFile (adminTokenEnvFile) only.
  forbiddenStoreConfigKeys = [
    "ADMIN_TOKEN"
    "SMTP_PASSWORD"
    "DATABASE_URL"
    "YUBICO_SECRET_KEY"
    "HIBP_API_KEY"
    "PUSH_INSTALLATION_KEY"
    "SSO_CLIENT_SECRET"
  ];

  extraConfigKeys = builtins.attrNames vw.extraConfig;
  forbiddenInExtraConfig = builtins.filter (
    k: builtins.elem k forbiddenStoreConfigKeys
  ) extraConfigKeys;

  # Final stock config after merge (includes host mkForce / other modules).
  finalVwConfig = config.services.vaultwarden.config or { };
  forbiddenInFinal = builtins.filter (
    k: finalVwConfig ? ${k} && finalVwConfig.${k} != null
  ) forbiddenStoreConfigKeys;

  # Private listen default: only 127.0.0.1 / ::1 unless operator opts out.
  rocketIsLoopback = vw.rocketAddress == "127.0.0.1" || vw.rocketAddress == "::1";
  rocketListenOk = rocketIsLoopback || vw.allowNonLoopbackListen;
in
{
  config = mkIf vwOn {
    assertions = [
      {
        assertion = vw.adminTokenEnvFile != "";
        message = ''
          surmount.vaultwarden.enable is true but adminTokenEnvFile is empty.
          Point it at a host EnvironmentFile that sets ADMIN_TOKEN=... under
          e.g. /var/lib/surmount/secrets/vaultwarden/admin.env (never in git).
          Install via nix run .#secrets-install-host. See docs/SECRETS.md.
        '';
      }
      {
        assertion = hostPaths.optionalStrictHostPath vw.adminTokenEnvFile;
        message = ''
          surmount.vaultwarden.adminTokenEnvFile must be empty or a strict
          absolute host path (/[A-Za-z0-9._/-]+, no metacharacters).
        '';
      }
      {
        assertion = !config.services.vaultwarden.configureNginx;
        message = ''
          surmount.vaultwarden refuses services.vaultwarden.configureNginx.
          Product edge is Axum-first; nginx is transitional-to-delete. Keep VW
          on loopback and expose later via operator tunnel / onion / future
          edge link only (management UI SURMOUNT_VAULTWARDEN_URL).
        '';
      }
      {
        assertion = forbiddenInExtraConfig == [ ];
        message = ''
          surmount.vaultwarden.extraConfig must not set secret-bearing keys
          (they land in the Nix store vaultwarden.env). Refused: ${lib.concatStringsSep ", " forbiddenInExtraConfig}.
          Put ADMIN_TOKEN / SMTP_PASSWORD / DATABASE_URL / similar only in a host
          EnvironmentFile (adminTokenEnvFile or services.vaultwarden.environmentFile).
          See docs/SECRETS.md.
        '';
      }
      {
        assertion = forbiddenInFinal == [ ];
        message = ''
          services.vaultwarden.config must not set secret-bearing keys
          (stock module writes them to a world-readable store EnvironmentFile).
          Refused: ${lib.concatStringsSep ", " forbiddenInFinal}.
          Use host EnvironmentFile paths only (never inline secrets in Nix config).
        '';
      }
      {
        assertion = rocketListenOk;
        message = ''
          surmount.vaultwarden.rocketAddress must be loopback (127.0.0.1 or ::1)
          unless allowNonLoopbackListen = true. Got: ${vw.rocketAddress}.
          Private single-VPS default is intentional; public bind is operator
          escape only (tunnel / onion / deliberate exposure).
        '';
      }
    ];

    # Stock package + SQLite; private listen; no public signup.
    services.vaultwarden = {
      enable = true;
      dbBackend = vw.dbBackend;
      # EnvironmentFile list: host-only ADMIN_TOKEN (and optional SMTP password).
      environmentFile = [ vw.adminTokenEnvFile ];
      configureNginx = false;
      configurePostgres = lib.mkDefault false;
      # Merge order: base // domain // extraConfig // forced private knobs.
      # Force SIGNUPS_ALLOWED false and ROCKET_* from Surmount options after
      # extraConfig so public open registration / world bind cannot slip in
      # via extraConfig without deliberate option + allowNonLoopbackListen.
      config =
        let
          # When path proxy is on and domain is empty, align DOMAIN with public subpath.
          proxyDomain =
            if cfg.managementUi.vaultwardenProxyEnable && vw.domain == "" then
              let
                p =
                  if lib.hasPrefix "/" cfg.managementUi.vaultwardenProxyPrefix then
                    lib.removeSuffix "/" cfg.managementUi.vaultwardenProxyPrefix
                  else
                    "/${lib.removeSuffix "/" cfg.managementUi.vaultwardenProxyPrefix}";
              in
              "https://${cfg.servicesHostname}${p}"
            else
              "";
          domainValue = if vw.domain != "" then vw.domain else proxyDomain;
        in
        {
          SIGNUPS_VERIFY = false;
          # stdout -> journald (unit StandardOutput). Never LOG_FILE (world-readable
          # store/state risk). Never ADMIN_TOKEN here.
          LOG_LEVEL = "info";
          EXTENDED_LOGGING = true;
        }
        // lib.optionalAttrs (domainValue != "") { DOMAIN = domainValue; }
        // vw.extraConfig
        // {
          SIGNUPS_ALLOWED = false;
          ROCKET_ADDRESS = vw.rocketAddress;
          ROCKET_PORT = vw.rocketPort;
        };
    };

    # Gate unit start on admin token file (missing = inactive, not thrash).
    # Mirrors management-ui PEM / directory token ConditionPathExists pattern.
    systemd.services.vaultwarden = {
      unitConfig = {
        ConditionPathExists = [ vw.adminTokenEnvFile ];
        StartLimitIntervalSec = 300;
        StartLimitBurst = 5;
      };
      serviceConfig = {
        # Align restart with other Surmount units (stock is Restart=always).
        Restart = lib.mkDefault "on-failure";
        RestartSec = lib.mkDefault "5s";
      };
    };

    # Document data path for operators (activation does not create secrets).
    environment.etc."surmount/vaultwarden-notes".text = ''
      # Surmount Vaultwarden (domain C human vault) -- generated notes
      # Data directory: ${dataDir}
      # Admin EnvironmentFile: ${vw.adminTokenEnvFile}
      # Rocket: ${vw.rocketAddress}:${toString vw.rocketPort}
      # Signups: disabled (SIGNUPS_ALLOWED=false)
      # Never put ADMIN_TOKEN or vault items in the public git tree.
      # Never put secret keys in surmount.vaultwarden.extraConfig (store-bound).
      # Backup: include ${dataDir} in restic paths when live (docs/OPS.md).
      # Not Bitwarden Secrets Manager; not deploy-secret activation for other units.
    '';
  };
}
