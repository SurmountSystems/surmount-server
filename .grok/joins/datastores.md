# Join: datastores architecture

**Date:** 2026-07-30
**Worker:** datastore architecture owner
**Status:** complete (docs + light module comments; no backend cutover)

## Delivered

| Artifact | What |
|----------|------|
| [`docs/DATASTORES.md`](../../docs/DATASTORES.md) | Main deep doc: Stalwart logical four-store model; directory vs data; blob CAS/BLAKE3; FTS internal vs ES/Meilisearch/PG/MySQL; backend comparison table (RocksDB, SQLite, PG, MySQL, FDB, S3, fs, Redis, ES, Meili); RocksDB LSM ops/backup/tuning knobs Stalwart actually exposes; **flake-eval verified** nixpkgs settings; spam-filter FOD vs message DB; import vs live store; **provisional RocksDB** decision framework + acceptance gates + open questions; **stack-wide inventory table**; backup/restore integrity |
| [`docs/SECRETS.md`](../../docs/SECRETS.md) | Three layers A/B/C (sops / Vaultwarden / LUKS); chicken-egg; anti-patterns (was referenced but missing) |
| [`docs/DECISIONS.md`](../../docs/DECISIONS.md) | **ADR-002 rewritten**: logical four-store **accepted**; physical all-RocksDB **accepted provisional** with rationale/gates/open questions (not "defaults are fine"). **ADR-016** added: inventory process pointer. Fork ADRs 004-005, 008, 012-015 were already present; left intact |
| [`docs/SECURITY.md`](../../docs/SECURITY.md) | Already existed; linked DATASTORES + ADR-016; RocksDB backup note |
| Cross-links | STACK, OPS, hygiene, SEARCH_AND_UI, README, secrets/README |
| [`modules/mail.nix`](../../modules/mail.nix) | Header + directory/store comments point at DATASTORES; store layout is explicit Surmount choice |
| [`modules/options.nix`](../../modules/options.nix) | `mailDataDir`/`stateDir` descriptions; comment stub for future `mailStore` backend options (not implemented) |

## Flake reality (verified)

```text
nix eval .#nixosConfigurations.mail-vps.config.services.stalwart-mail.settings
store.db = { type = rocksdb; path = /var/lib/stalwart-mail/db; compression = lz4 }
storage.{data,fts,lookup,blob} = "db"
directory.internal = { type = internal; store = db }
spam-filter.resource = Surmount FOD (not message DB)
stateVersion 25.05 => non-legacy path (not SQLite+fs blobs)
```

nixpkgs module source (locked): `stalwart-mail.nix` at rev `ac62194c...`

## Intentional tone correction

Prior "Store config left at RocksDB defaults (already correct)" is **rejected**.
nixpkgs is a proposal; ADR-002 + DATASTORES document **why** we provisionally
co-locate, **when** to leave, and **what** is still open for a DB engineer.

## Out of scope (as requested)

- Postgres cutover implementation
- Production migration run
- Full Nostr/Vaultwarden code

## Open for operator

See DATASTORES.md section 6.4: Postgres for data+directory; split blobs;
external FTS; bufferSize/poolWorkers; RPO/RTO; VW SQLite vs PG; product
SQLite under `/var/lib/surmount`.

## Suggested next (not started)

- Set RPO/RTO when MX goes live; first timed restore drill
- When VW module lands: inventory row + restic path + SECRETS wire-up
- Optional: expose `store.db.bufferSize` / `poolWorkers` after RAM class known
