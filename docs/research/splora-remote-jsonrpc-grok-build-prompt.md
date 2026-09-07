# Grok Build prompt: remote JSON-RPC for Splora REST

Paste the block under **Prompt** into a Grok Build session whose working
tree is the **splora** clone (`/home/hunter/Projects/surmount/splora`,
fork of mempool electrs on the `surmount` branch). This file is the
durable copy. Do not run that work from a surmount-server session.

The mempool / Esplora REST is the indexer binary. It can talk to a
remote or already-running bitcoind over JSON-RPC. A local bitcoind
datadir on the mail guest is not an API requirement.

## Prompt

```
You are implementing in this tree: splora (Surmount take of mempool electrs).
Working directory is the splora checkout, not surmount-server.

Product outcome
---------------
The Esplora-compatible REST (mempool API) on the indexer binary must be
able to run against a remote or already-running bitcoind over JSON-RPC
without a local bitcoind datadir on the same machine.

The NixOS module today always passes --daemon-dir (default
/var/lib/bitcoind) and does not pass --jsonrpc-import. systemd
ReadOnlyPaths then fails if that path is missing. That is a module
bug for remote RPC, not an API requirement.

Do this
-------
1. First-class module options (thoughtful names, complete descriptions):
   daemonRpcAddr, cookieFile (path only), jsonrpcImport (bool).
   When cookieFile and daemonRpcAddr are set, daemonDir must be optional.
   systemd must not ReadOnlyPaths a missing datadir.
2. Pass --jsonrpc-import when that option is true. Help text already
   says it is useful for a remote full node.
3. One instance is enough. Do not require five networks. Do not default
   dbBlockCacheMb to 4096 as if that were required for REST. Keep CLI
   default 24 unless the operator sets more. Optional MemoryMax on the
   indexer unit, default off / unset in public sample.
4. Optional publicHealth bool that passes --public-health so
   GET /blocks/tip/height can work without NIP-98. Empty allowlist still
   401s address/tx/mempool.
5. Tests in the existing Nix eval / rust tests: remote RPC config does
   not require daemonDir; cookie bytes never appear in the module.
6. Docs: mempool REST does not need Let's Encrypt vhosts, HTTP/3,
   hypervisor UDP 443, grok-oss, or queue-only enable. Queue is not REST.

Do not
------
Do not add Electrum TCP. Do not start bitcoind from this module.
Do not put secrets in git. Do not invent HTTP/3 in this tree.
Do not change the Menhera laptop cargo rewrite. Crane src still omits
.cargo/config.toml and builds --offline --locked.

Acceptance
----------
A NixOS eval with one instance, jsonrpcImport = true, daemonRpcAddr
set, cookieFile set, daemonDir unset, starts splora-mainnet (or the
named instance) ExecStart containing --jsonrpc-import and
--daemon-rpc-addr and --cookie-file, and does not list a missing
/var/lib/bitcoind in ReadOnlyPaths.
```
