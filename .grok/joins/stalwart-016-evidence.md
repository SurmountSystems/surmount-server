# Join: Stalwart 0.16.15 stores evidence

**Date:** 2026-07-30
**Status:** research complete. Docs only. No package version changes.
**Not** operator acceptance of store layout.

## Deliverable

Full writeup:

`docs/research/stalwart-0.16.15-stores-evidence.md`

Historical 0.11.8 only (do not use as current):

`docs/research/stalwart-stores-evidence-2026-07-30.md`

## Versions verified

| Component | Version | How | `--version` / eval |
|-----------|---------|-----|--------------------|
| server | **0.16.15** | release binary FOD | `stalwart --version` -> `0.16.15` |
| cli | **1.0.12** | release binary FOD | `stalwart-cli 1.0.12` |
| webui | **1.0.7** | zip FOD | package version |
| spam-filter | **3.0.0** | toml + rules FODs | package version |
| packagingMode | `release-binary-fod` | passthru | eval green |
| nixpkgs OS | nixos-25.05 rev `ac62194c3917d5f474c1a844b6fd6da2db95077d` | flake.lock | engine **not** from channel |

## Storage model (0.16)

- Four roles still: DataStore, BlobStore, SearchStore, InMemoryStore.
- On disk: only `config.json` = DataStore JSON. Surmount generates:
  `{"@type":"RocksDb","path":"/var/lib/stalwart-mail/db"}`
- Other roles + listeners + accounts: JMAP objects via WebUI / `stalwart-cli apply`.
- Single-node docs: RocksDB for all four is normal; RocksDB recommended for single-node data.
- Backends: Rocks/FDB/PG/MySQL/SQLite multi-role; S3/Azure/FS blob; ES + **Meilisearch** search; Redis in-memory.
- RocksDB JSON fields: `path`, `blobSize` default **16834**, `bufferSize` default **134217728**, `poolWorkers`.
- Blob hash still **BLAKE3** (32-byte); source `BlobHash::generate` + docs.
- FTS Default = internal (docs: bloom filters); ES and Meilisearch external.
- Directory: internal (in data store), LDAP, SQL, OIDC.

## Source crate pins (tag v0.16.15 Cargo.lock)

rocksdb 0.24.0, librocksdb-sys 0.17.3+10.4.2, blake3 1.8.5, store crate 0.16.15.

## Surmount module

- `modules/stalwart-service.nix` disables nixpkgs TOML module; runs `stalwart --config=...`.
- `modules/mail.nix`: RocksDb path under mailDataDir; FODs at `/etc/surmount/stalwart/` not auto-wired.
- blobSize/bufferSize left at upstream defaults.

## Unknowns (see evidence section 4)

Live first-boot singleton JSON not captured; FOD exact feature list vs Dockerfile inferred; full VM test not run; import CLI subcommand not re-checked against `--help`.

## Sources (short)

- https://stalw.art/docs/storage/ and install/store, backends, data, blob, fts, in-memory, rocksdb
- https://stalw.art/docs/ref/object/data-store
- https://github.com/stalwartlabs/stalwart/blob/v0.16.15/UPGRADING/v0_16.md
- Tag source: crates/store, types/blob_hash.rs, Dockerfile features
- Local: nix/packages/stalwart-*.nix, modules/stalwart-service.nix, modules/mail.nix, flake.lock
