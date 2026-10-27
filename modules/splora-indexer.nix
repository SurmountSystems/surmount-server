# Wrap imported services.splora for one remote JSON-RPC indexer.
# Host-local single knob. Sample host stays off. Cookie bytes never belong here.
# Imported instance options (be3603d) already have cookieFile, daemonRpcAddr,
# jsonrpcImport, publicHealth, daemonDir = null, db cache 24, and
# httpSocketFile /run/splora/${name}.http.sock. This wrap still maps one
# instance, asserts cookie path charset, and does not start five indexers.
# Instance and unit names are static (mkIf per allowed name) so eval does
# not interpolate config into attr keys.

{
  config,
  lib,
  options,
  ...
}:
let
  inherit (lib)
    mkIf
    mkMerge
    optionals
    ;
  hostPaths = import ./lib/host-paths.nix { inherit lib; };
  cfg = config.surmount;
  idx = cfg.sploraIndexer;
  hasSplora = options.services ? splora;
  allowedNames = [
    "mainnet"
    "testnet3"
    "testnet4"
    "mutinynet"
    "liquid"
  ];
in
{
  config = mkMerge (
    [
      (mkIf cfg.enable {
        assertions = [
          {
            assertion = !idx.enable || hasSplora;
            message = ''
              surmount.sploraIndexer.enable requires the imported
              nixosModules.splora (services.splora).
            '';
          }
          {
            assertion = !idx.enable || builtins.elem idx.instanceName allowedNames;
            message = ''
              surmount.sploraIndexer.instanceName must be mainnet, testnet3,
              testnet4, mutinynet, or liquid.
            '';
          }
          {
            assertion = !idx.enable || idx.daemonRpcAddr != "";
            message = ''
              surmount.sploraIndexer.enable requires daemonRpcAddr (JSON-RPC
              host:port passed as --daemon-rpc-addr). Host-local.
            '';
          }
          {
            assertion = !idx.enable || hostPaths.strictHostPath idx.cookieFile;
            message = ''
              surmount.sploraIndexer.cookieFile must be a strict absolute host
              path (cookie file path only; never cookie bytes).
            '';
          }
          {
            assertion = !idx.enable || idx.daemonDir == null || hostPaths.strictHostPath idx.daemonDir;
            message = ''
              surmount.sploraIndexer.daemonDir must be null (remote JSON-RPC)
              or a strict absolute host path.
            '';
          }
        ];
      })
      # Nice on units this module's imported splora stack actually starts.
      # Static instance names. Do not map config.services.splora.instances
      # into systemd attr keys (that recurses). Instance enable defaults
      # true even when services.splora.enable is false, so the service
      # enable is required or those names become Nice-only stubs.
      (mkIf (hasSplora && config.services.splora.enable) {
        systemd.services = mkMerge (
          map (
            name:
            let
              inst = config.services.splora.instances.${name} or { };
              on = inst.enable or false;
              daemonName = if name == "liquid" then "elementsd-liquid" else "bitcoind-${name}";
            in
            mkMerge [
              (mkIf on {
                "splora-${name}" = {
                  serviceConfig.Nice = 5;
                };
              })
              (mkIf (on && (inst.startLocalDaemon or false)) {
                ${daemonName} = {
                  serviceConfig.Nice = 5;
                };
              })
            ]
          ) allowedNames
        );
      })
    ]
    ++ optionals hasSplora (
      [
        (mkIf (cfg.enable && idx.enable) {
          services.splora.enable = true;
          services.splora.instances = mkMerge (
            map (name: {
              ${name} = mkIf (idx.instanceName == name) {
                enable = true;
                network = idx.network;
                daemonRpcAddr = idx.daemonRpcAddr;
                cookieFile = idx.cookieFile;
                daemonDir = idx.daemonDir;
                jsonrpcImport = idx.jsonrpcImport;
                publicHealth = idx.publicHealth;
                dbBlockCacheMb = idx.dbBlockCacheMb;
                extraArgs = idx.extraArgs;
              };
            }) allowedNames
          );
        })
      ]
    )
  );
}
