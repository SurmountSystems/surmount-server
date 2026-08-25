# Surmount-owned Stalwart 0.16+ service module.
#
# Why not stock nixpkgs Stalwart modules?
#   nixos-25.x: services/mail/stalwart-mail.nix (TOML + bin/stalwart-mail).
#   nixos-26.05+: services/mail/stalwart.nix renames options to services.stalwart
#   and still targets TOML / older packaging. Stalwart 0.16 dropped TOML: on-disk
#   config is a small config.json (DataStore only). Everything else (listeners,
#   accounts, spam, TLS) lives in the datastore as JMAP objects and is managed
#   via WebUI or stalwart-cli apply.
#   Upstream notes: resources/UPGRADING/v0_16.md in the stalwart tag we pin.
#
# Approach: claim the stock option name services.stalwart and dual-disable both
# stock module paths so they do not conflict. Surmount still owns 0.16 config.json
# generation; we are not adopting the stock TOML module body. Unit name stays
# stalwart-mail.service (and /var/lib/stalwart-mail, user/group) for state and
# ops stability; binary is `stalwart`.

{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.stalwart;
  hostPaths = import ./lib/host-paths.nix { inherit lib; };
  # 0.16 DataStore JSON: tagged union with @type. Defaults for blobSize /
  # bufferSize match upstream RocksDbStore::default when omitted.
  configJson =
    if cfg.configFile != null then
      cfg.configFile
    else
      pkgs.writeText "stalwart-config.json" (
        builtins.toJSON (
          {
            "@type" = cfg.storeType;
            path = cfg.storePath;
          }
          // lib.optionalAttrs (cfg.blobSize != null) { blobSize = cfg.blobSize; }
          // lib.optionalAttrs (cfg.bufferSize != null) { bufferSize = cfg.bufferSize; }
        )
      );

  # Path-only EnvironmentFile for recovery pin. Leading "-" = ignore if missing
  # so the unit still starts after strip hygiene (no recovery.env on disk).
  recoveryEnvFiles = lib.optional (cfg.recoveryAdminEnvFile != "") "-${cfg.recoveryAdminEnvFile}";
in
{
  # Replace stock nixpkgs Stalwart modules entirely.
  # 25.05 path + 26.05 path. We claim services.stalwart; disable stock so
  # neither TOML module conflicts with our options or unit.
  disabledModules = [
    "services/mail/stalwart-mail.nix"
    "services/mail/stalwart.nix"
  ];

  options.services.stalwart = {
    enable = lib.mkEnableOption "Stalwart mail and collaboration server (Surmount 0.16+ module)";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.stalwart-mail;
      defaultText = lib.literalExpression "pkgs.stalwart-mail";
      description = "Stalwart package (must provide bin/stalwart for 0.16+).";
    };

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/stalwart-mail";
      description = "State directory root (RocksDB path defaults under this).";
    };

    storeType = lib.mkOption {
      type = lib.types.enum [
        "RocksDb"
        "Sqlite"
      ];
      default = "RocksDb";
      description = ''
        On-disk DataStore @type written into config.json.
        Other backends (PostgreSQL, MySQL, FoundationDB) need a hand-written
        configFile; this option only covers local single-node defaults.
      '';
    };

    storePath = lib.mkOption {
      type = lib.types.path;
      default = "${cfg.dataDir}/db";
      defaultText = lib.literalExpression ''"''${config.services.stalwart.dataDir}/db"'';
      description = "Filesystem path for RocksDB / SQLite store.";
    };

    blobSize = lib.mkOption {
      type = lib.types.nullOr lib.types.ints.unsigned;
      default = null;
      description = "Optional RocksDB blobSize override (upstream default 16834).";
    };

    bufferSize = lib.mkOption {
      type = lib.types.nullOr lib.types.ints.unsigned;
      default = null;
      description = "Optional RocksDB bufferSize override (upstream default 134217728).";
    };

    configFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        If set, use this path as --config instead of generating config.json
        from storeType/storePath. Must be a 0.16 DataStore JSON document.
      '';
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Open a conservative set of mail / collab ports. Prefer networking.nix
        for an auditable Surmount port list; leave false there.
      '';
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "stalwart-mail";
      description = "System user for the service.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "stalwart-mail";
      description = "System group for the service.";
    };

    credentials = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = ''
        systemd LoadCredential map. Values are host paths; names appear under
        /run/credentials/stalwart-mail.service/.
      '';
      example = {
        admin_password = "/run/keys/stalwart_admin_password";
      };
    };

    extraEnvironment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = ''
        Extra environment for the unit (non-secret only). Useful keys include
        STALWART_HOSTNAME, STALWART_PUBLIC_URL, STALWART_RECOVERY_MODE.
        Never put STALWART_RECOVERY_ADMIN (or any password) here: it would
        land in the Nix store. Use recoveryAdminEnvFile (path only) for a
        durable recovery pin, or temporary private systemd drop-in from
        nix run .#stalwart-recovery-unlock. Prefer strip after permanent
        admin API key (hygiene).
      '';
    };

    recoveryAdminEnvFile = lib.mkOption {
      type = lib.types.str;
      default = "";
      example = "/var/lib/surmount/secrets/stalwart/recovery.env";
      description = ''
        Host path to a systemd EnvironmentFile that may set
        STALWART_RECOVERY_ADMIN=user:password (KEY=value lines; mode 0600;
        never in git). Path only is declared in the unit so reboot survives
        while the pin is intentionally kept. Empty (default) = no unit
        EnvironmentFile for recovery: prefer strip after permanent admin
        API key, or temporary unlock drop-in. Never put the password body
        in Nix or extraEnvironment. Unit uses EnvironmentFile=-path so a
        missing file after strip does not fail the unit. Install material
        via nix run .#stalwart-recovery-unlock / secrets-install-host
        (kind stalwart-recovery-admin). See docs/OPS.md, docs/SECRETS.md.
      '';
    };

    # Kept so older mail.nix references to settings do not explode during
    # migration. 0.16 ignores TOML; settings are not written to disk here.
    settings = lib.mkOption {
      type = lib.types.attrs;
      default = { };
      description = ''
        Deprecated for 0.16+. Previously fed a TOML config. Surmount now
        generates config.json only. Use WebUI / stalwart-cli apply for
        listeners, spam, TLS, accounts. This attrset is accepted and ignored
        so older fragments do not fail evaluation.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.storeType == "RocksDb" || cfg.storeType == "Sqlite" || cfg.configFile != null;
        message = "services.stalwart: unsupported storeType without configFile";
      }
      {
        assertion = hostPaths.optionalStrictHostPath cfg.recoveryAdminEnvFile;
        message = ''
          services.stalwart.recoveryAdminEnvFile must be empty or a strict
          absolute host path (/[A-Za-z0-9._/-]+, no metacharacters).
          Path only; never put STALWART_RECOVERY_ADMIN password in Nix.
        '';
      }
      {
        # Password body must never land in the store via Environment=.
        assertion = !(cfg.extraEnvironment ? STALWART_RECOVERY_ADMIN);
        message = ''
          services.stalwart.extraEnvironment must not set STALWART_RECOVERY_ADMIN
          (secret would enter the Nix store). Use recoveryAdminEnvFile for a
          host EnvironmentFile path only, or temporary unlock drop-in from
          nix run .#stalwart-recovery-unlock. Prefer strip after permanent
          admin API key.
        '';
      }
      {
        # Defense in depth: refuse direct unit environment too (host profile
        # can set systemd.services.stalwart-mail.environment without going
        # through extraEnvironment). Path-only EnvironmentFile is the only
        # supported recovery pin load for this secret.
        assertion = !((config.systemd.services.stalwart-mail.environment or { }) ? STALWART_RECOVERY_ADMIN);
        message = ''
          systemd.services.stalwart-mail.environment must not set
          STALWART_RECOVERY_ADMIN (secret would enter the Nix store). Use
          services.stalwart.recoveryAdminEnvFile for a host EnvironmentFile
          path only, or temporary unlock drop-in from
          nix run .#stalwart-recovery-unlock. Prefer strip after permanent
          admin API key.
        '';
      }
    ];

    users.groups = lib.mkIf (cfg.group == "stalwart-mail") {
      stalwart-mail = { };
    };
    users.users = lib.mkIf (cfg.user == "stalwart-mail") {
      stalwart-mail = {
        isSystemUser = true;
        group = cfg.group;
      };
    };

    systemd.tmpfiles.rules = [
      "d '${cfg.dataDir}' 0750 ${cfg.user} ${cfg.group} - -"
      "d '${cfg.storePath}' 0750 ${cfg.user} ${cfg.group} - -"
    ];

    systemd.services.stalwart-mail = {
      description = "Stalwart Mail and Collaboration Server";
      wantedBy = [ "multi-user.target" ];
      after = [
        "local-fs.target"
        "network.target"
      ];

      environment = {
        STALWART_HOSTNAME = lib.mkDefault config.networking.hostName;
        # stdout tracer -> journald (SyslogIdentifier=stalwart-mail). Never put
        # recovery passwords or API tokens in extraEnvironment.
        RUST_LOG = lib.mkDefault "info";
      }
      // cfg.extraEnvironment;

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        ExecStart = "${lib.getExe cfg.package} --config=${configJson}";
        LimitNOFILE = 65536;
        KillMode = "process";
        KillSignal = "SIGINT";
        Restart = "on-failure";
        RestartSec = 5;
        SyslogIdentifier = "stalwart-mail";
        StandardOutput = "journal";
        StandardError = "journal";
        LoadCredential = lib.mapAttrsToList (key: value: "${key}:${value}") cfg.credentials;

        ReadWritePaths = [ cfg.dataDir ];
        CacheDirectory = "stalwart-mail";
        StateDirectory = "stalwart-mail";

        AmbientCapabilities = [ "CAP_NET_BIND_SERVICE" ];
        CapabilityBoundingSet = [ "CAP_NET_BIND_SERVICE" ];

        DeviceAllow = [ "" ];
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        PrivateDevices = true;
        PrivateUsers = false;
        ProcSubset = "pid";
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [
          "@system-service"
          "~@privileged"
        ];
        UMask = "0077";
      }
      # Path-only EnvironmentFile for recovery pin (never password body).
      // lib.optionalAttrs (recoveryEnvFiles != [ ]) {
        EnvironmentFile = recoveryEnvFiles;
      };
    };

    environment.systemPackages = [
      cfg.package
    ];

    # Conservative mail-plane ports only. Deliberately omits 80/443: product
    # clearnet HTTPS is management-ui (Axum), not Stalwart. Prefer leave
    # openFirewall false; modules/networking.nix owns production firewall.
    # 8080 listed only if you opt into openFirewall (not recommended public).
    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [
      25
      465
      587
      993
      995
      4190
      8080
    ];
  };
}
