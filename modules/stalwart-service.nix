# Surmount-owned Stalwart 0.16+ service module.
#
# Why not nixpkgs services.stalwart-mail (25.05)?
#   That module generates TOML and runs bin/stalwart-mail. Stalwart 0.16 dropped
#   TOML: on-disk config is a small config.json (DataStore only). Everything
#   else (listeners, accounts, spam, TLS) lives in the datastore as JMAP objects
#   and is managed via WebUI or stalwart-cli apply.
#   Upstream notes: resources/UPGRADING/v0_16.md in the stalwart tag we pin.
#
# Why not unstable services.stalwart?
#   Still targets the 0.15 TOML world; nixpkgs warns stalwart_0_16 is not
#   compatible with that module.
#
# Approach: disable the nixpkgs module and run our binary with generated
# config.json. Unit name stays stalwart-mail.service for less churn in tests
# and ops notes; binary is `stalwart`.

{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.stalwart-mail;
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
in
{
  # Replace the nixpkgs 25.05 TOML module entirely.
  disabledModules = [ "services/mail/stalwart-mail.nix" ];

  options.services.stalwart-mail = {
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
      defaultText = lib.literalExpression ''"''${config.services.stalwart-mail.dataDir}/db"'';
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
        Extra environment for the unit. Useful keys include:
        STALWART_HOSTNAME, STALWART_PUBLIC_URL, STALWART_RECOVERY_MODE,
        STALWART_RECOVERY_ADMIN=admin:password (bootstrap pin).
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
        message = "services.stalwart-mail: unsupported storeType without configFile";
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
      };
    };

    environment.systemPackages = [
      cfg.package
    ];

    # Conservative port set matching upstream 0.16 first-boot defaults.
    # Surmount networking.nix should still own production firewall policy.
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
