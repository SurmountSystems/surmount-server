# Historical evidence: Stalwart four-store model (0.11.8)

**Status:** **historical 0.11.8 evidence only.** Research finding; not
operator-accepted architecture. **Date:** 2026-07-30

**Context:** The scaffold briefly shipped nixpkgs `stalwart-mail` **0.11.8**
from nixos-25.05. That was an accident of the channel pin, not a product
choice. Stalwart is **being moved to current**. Re-gather store evidence after
the package bump (`flake.nix`, `nix/packages/`, `modules/mail.nix`). Until
then, treat numbers and paths below as a snapshot of the old package, not the
target engine.

This file pins **versions, paths, and source symbols** for the **0.11.8**
store model only. Do not treat scaffold nixpkgs defaults as a Surmount product
decision. Do not read this as "version skew is unsafe"; it is historical fact.

## 1. What binary we actually build

| Item | Value | Where |
|------|--------|--------|
| nixpkgs channel (flake input) | `nixos-25.05` | `flake.nix` / `flake.lock` |
| nixpkgs git rev (locked) | `ac62194c3917d5f474c1a844b6fd6da2db95077d` | `flake.lock` node `nixpkgs` |
| nixpkgs store path (this machine) | `/nix/store/hs7sfwdsiqcfrfaj620r8cjjnscb09k9-3p306srz83h9z9v0ma9xcxb8y8cdxkxj-source` | `nix eval ...pkgs.path` |
| Package attr | `pkgs.stalwart-mail` | nixpkgs by-name |
| Package name-version | `stalwart-mail-0.11.8` | `nix eval ...stalwart-mail.name` |
| Package semver | **0.11.8** | nixpkgs `package.nix` |
| Latest upstream GitHub release (checked 2026-07-30) | **v0.16.15** (published 2026-07-27) | `api.github.com/repos/stalwartlabs/mail-server/releases/latest` |
| Is 0.11.8 latest? | **No.** About five minor lines behind latest tag. | |
| Upstream repo | https://github.com/stalwartlabs/mail-server | |
| License (community) | AGPL-3.0-only (+ optional enterprise SEL) | package.nix meta |
| NixOS module option | `services.stalwart-mail` | module path below |
| Source tag fetched | `v0.11.8` | package.nix `fetchFromGitHub` |
| Realized src store path | `/nix/store/0d0p9rj2gp5rjhcfq2vws9bw3b6q8r34-source` | `nix build ...stalwart-mail.src` |

### Package definition (nixpkgs)

- File: `pkgs/by-name/st/stalwart-mail/package.nix`
- Absolute (locked nixpkgs):
  `/nix/store/hs7sfwdsiqcfrfaj620r8cjjnscb09k9-3p306srz83h9z9v0ma9xcxb8y8cdxkxj-source/pkgs/by-name/st/stalwart-mail/package.nix`
- Build: `rustPlatform.buildRustPackage`
- `version = "0.11.8"`
- Cargo features enabled by nixpkgs (not default cargo features):
  `sqlite`, `postgres`, `mysql`, `rocks`, `elastic`, `s3`, `redis`
  optional: `foundationdb`, `enterprise`
- System RocksDB linked via env: `ROCKSDB_INCLUDE_DIR`, `ROCKSDB_LIB_DIR`
- System RocksDB package on this eval: **rocksdb 10.2.1** (`pkgs.stalwart-mail.rocksdb`)

### NixOS module (nixpkgs)

- File: `nixos/modules/services/mail/stalwart-mail.nix`
- Absolute:
  `/nix/store/hs7sfwdsiqcfrfaj620r8cjjnscb09k9-3p306srz83h9z9v0ma9xcxb8y8cdxkxj-source/nixos/modules/services/mail/stalwart-mail.nix`
- Scaffold defaults when `stateVersion >= 24.11`:
  `store.db.type = "rocksdb"`, path `${dataDir}/db`, compression `lz4`,
  `storage.{data,fts,lookup,blob} = "db"`
- Legacy (`stateVersion < 24.11`): sqlite data + filesystem blobs

### Surmount wiring (this repo; not a product acceptance)

- `modules/mail.nix` enables `services.stalwart-mail` and overrides spam-filter FOD
- Eval shows roles all pointing at id `db` (rocksdb). That is **module default inheritance**, not an approved Surmount datastore design unless you accept it.

## 2. Upstream workspace (v0.11.8 source)

Root: `/nix/store/0d0p9rj2gp5rjhcfq2vws9bw3b6q8r34-source`

| Workspace crate | path | Cargo package version |
|-----------------|------|------------------------|
| `store` | `crates/store/` | **0.11.8** |
| `directory` | `crates/directory/` | **0.11.8** |
| `mail-server` (bin workspace) | workspace root / `crates/main` | **0.11.8** |
| `utils` | `crates/utils/` | **0.11.8** |
| `nlp` (tokenization/lang for FTS) | `crates/nlp/` | **0.11.8** |

`crates/store/Cargo.toml` declares `name = "store"`, `version = "0.11.8"`.
This is an **internal path crate**, not published as a public crates.io product API for Surmount to depend on.

### Third-party crates (from v0.11.8 `Cargo.lock`)

| Crate | Locked version | Role |
|-------|----------------|------|
| `rocksdb` (Rust crate) | **0.23.0** | RocksDB bindings (`features = ["multi-threaded-cf"]` in store Cargo.toml) |
| `librocksdb-sys` | **0.17.1+9.9.3** | Bundled/sys layer in lock (nixpkgs **overrides** link to system RocksDB **10.2.1**) |
| `blake3` | **1.5.5** locked (Cargo.toml dep `"1.3.3"`) | Content hash for blobs |
| `lz4_flex` | **0.11.3** | Optional blob payload compression (app-level) |
| `roaring` | (via store) | Bitmap sets for FTS/query |
| `bitpacking` | (via store) | FTS postings packing |
| `rusqlite` | **0.32.1** | SQLite backend (feature `sqlite`) |
| `tokio-postgres` | **0.7.12** | PostgreSQL backend (feature `postgres`) |
| `mysql_async` | **0.34.1** | MySQL backend |
| `foundationdb` | **0.9.2** | FDB backend (feature off in default nixpkgs unless `withFoundationdb`) |
| `rust-s3` | **0.35.0-alpha.2** | S3 blob backend |
| `elasticsearch` | **8.5.0-alpha.1** | External FTS backend |
| `redis` | **0.27.6** | In-memory/lookup backend |

**Not in v0.11.8 `store` crate:** Meilisearch client. Do not assume Meilisearch from newer docs applies to 0.11.8 without checking a newer tag.

## 3. Four store roles (code, not marketing)

In `crates/store/src/lib.rs` the runtime splits into **four typed maps**:

```text
Stores {
  stores:            AHashMap<String, Store>,         // data-capable backends
  blob_stores:       AHashMap<String, BlobStore>,
  fts_stores:        AHashMap<String, FtsStore>,
  in_memory_stores:  AHashMap<String, InMemoryStore>,
}
```

Config selectors (TOML keys used by 0.11.8):

| Role | Config key | Types enum | File |
|------|------------|------------|------|
| Data | `storage.data` | `Store` | `crates/store/src/lib.rs`, `crates/store/src/config.rs`, `crates/common/src/config/mod.rs` |
| Blob | `storage.blob` | `BlobStore` / `BlobBackend` | same |
| FTS / search | `storage.fts` | `FtsStore` | same |
| Lookup / in-memory | `storage.lookup` | `InMemoryStore` | same |

Backend **instances** are declared under `store.<id>.type = ...` then referenced by id from `storage.*`.

### 3.1 Data (`Store`)

**Enum variants** (`crates/store/src/lib.rs`):

- `RocksDb` (feature `rocks`)
- `SQLite` (feature `sqlite`)
- `PostgreSQL` (feature `postgres`)
- `MySQL` (feature `mysql`)
- `FoundationDb` (feature `foundation`)
- `SQLReadReplica` (enterprise + sql)
- `None`

**Config type strings** (`crates/store/src/config.rs`): `"rocksdb"`, `"postgresql"`, `"sqlite"`, `"mysql"`, etc.

**Holds (engine responsibility):** structured metadata: mailbox state, folders/flags style data, settings, queues subspaces, directory subspace when internal directory points here, FTS index subspace when FTS is internal, etc. Implemented via subspaces (u8 tags), e.g.:

| Subspace const | byte | Name in code |
|----------------|------|----------------|
| `SUBSPACE_DIRECTORY` | `b'd'` | directory rows |
| `SUBSPACE_PROPERTY` | `b'p'` | properties |
| `SUBSPACE_FTS_INDEX` | `b'g'` | internal FTS |
| `SUBSPACE_BLOBS` | `b't'` | blob CF name also |
| `SUBSPACE_IN_MEMORY_VALUE` | `b'm'` | durable lookup values |
| `SUBSPACE_QUEUE_*` | `e`/`q` | queues |
| ... | | see `lib.rs` 134-158 |

RocksDB open path: `crates/store/src/backend/rocksdb/main.rs`
Uses `OptimisticTransactionDB` with many column families (one per subspace family).

### 3.2 Blob (`BlobStore` / `BlobBackend`)

**Enum** (`lib.rs`):

- `BlobBackend::Store(Store)` -- blobs inside a data backend
- `BlobBackend::Fs`
- `BlobBackend::S3` (feature `s3`)
- `BlobBackend::Azure` (feature `azure`; **not** in nixpkgs `buildFeatures` list)
- `BlobBackend::Sharded` (enterprise)

**Content addressing:**

- Type: `BlobHash([u8; BLOB_HASH_LEN])` with `BLOB_HASH_LEN = 32`
- File: `crates/utils/src/lib.rs`
- Hash: `blake3::hash(value)` (`impl From<&[u8]> for BlobHash`)
- Crate: `blake3` **1.5.5** locked

**App-level blob compression** (separate from RocksDB compression):

- `CompressionAlgo::{None, Lz4}` in `lib.rs`
- LZ4 via `lz4_flex` in `crates/store/src/dispatch/blob.rs`
- Per-store config key: `store.<id>.compression` default **`"none"`** in `config.rs`
  (nixpkgs still sets `store.db.compression = "lz4"` at the **store** level for RocksDB; treat as RocksDB/store setting, and verify interaction when choosing layout)

**RocksDB integrated blob files threshold (not the same as "logical inline"):**

- File: `crates/store/src/backend/rocksdb/main.rs`
- `cf_opts.set_enable_blob_files(true)`
- `set_min_blob_size(config property mini-key **`min-blob-size`**, default string **`"16834"`**)`
- This is RocksDB BlobDB-style SST vs blob-file split for the blobs column family.
- Official product docs also describe small objects staying in the data store vs spilling to blob backend; that is a **layer above** and should be verified separately per version. Do not conflate `min-blob-size` with every doc phrase about "inline."

**Write buffer default (RocksDB):** `write-buffer-size` default **134217728** (128 MiB) in same `main.rs`.

### 3.3 FTS (`FtsStore`)

**Enum:**

- `FtsStore::Store(Store)` -- internal index inside a data backend
- `FtsStore::ElasticSearch` (feature `elastic`)

**Internal implementation (0.11.8):**

- Module: `crates/store/src/fts/` (`index.rs`, `query.rs`, `postings.rs`)
- Uses **token fields** (Header/Body/Attachment/Keyword), language detection via `nlp` crate
- Query side uses **`roaring::RoaringBitmap`**
- Postings use **`bitpacking`**
- Index entries as `ValueClass::FtsIndex` / subspace `SUBSPACE_FTS_INDEX` (`b'g'`)

**Precision note:** Earlier Surmount prose said "bloom-filter style." In **0.11.8 source**, the visible structures are **token hashes + roaring bitmaps + bitpacked postings**, not a classic Bloom-filter API. Prefer that wording unless a specific Bloom type appears in a cited file.

**Elasticsearch:** optional external FTS; crate `elasticsearch` **8.5.0-alpha.1**.

### 3.4 Lookup / in-memory (`InMemoryStore`)

**Enum:**

- `InMemoryStore::Store(Store)` -- can be durable in RocksDB/SQL subspaces
- `InMemoryStore::Redis`
- `InMemoryStore::Http`
- `InMemoryStore::Static`
- `InMemoryStore::Sharded` (enterprise)

Config key: `storage.lookup`. Soft state (rate limits, tokens, etc.) per upstream docs; when backed by durable `Store`, expired keys need purge schedules.

## 4. Directory (related, not a fifth storage role)

- Crate: `crates/directory` version **0.11.8**
- Internal directory: `DirectoryInner::Internal(Store)` in `crates/directory/src/lib.rs`
- Config type `"internal"` in `crates/directory/src/core/config.rs`
- nixpkgs default: `directory.internal.type = "internal"`, `directory.internal.store = "db"`, `storage.directory = "internal"`

Mail account principals live in directory; with internal directory they sit in the **data** store backend.

## 5. Scaffold config currently evaluated on mail-vps

```text
store.db.type = "rocksdb"
store.db.path = "/var/lib/stalwart-mail/db"
store.db.compression = "lz4"
storage.data = "db"
storage.blob = "db"
storage.fts = "db"
storage.lookup = "db"
directory.internal.store = "db"
```

Source of those defaults: nixpkgs `stalwart-mail.nix` lines setting `mkDefault`, plus Surmount `modules/mail.nix` not overriding store layout.

**This is not operator acceptance.**

## 6. Historical package note (0.11.8 vs then-current upstream)

| Topic | 0.11.8 (scaffold / nixos-25.05) | Upstream at evidence date (v0.16.x) |
|-------|--------------------------------|-------------------------------------|
| Config surface | TOML `store.*` / `storage.*` | Docs emphasize WebUI objects DataStore/BlobStore/SearchStore/InMemoryStore |
| Package in nixos-25.05 | 0.11.8 | GitHub latest was v0.16.15 (2026-07-30) |
| Meilisearch | not in store Cargo.toml features | may exist on newer tags; verify on **current** package |
| Upgrade | changing major store layout may need migration (see upstream UPGRADING.md) | |

Surmount tracks **current** Stalwart (packaging bump in flight). nixpkgs lag is
not a reason to stay on 0.11.8. This section is historical evidence only.

## 7. Open for operator (explicitly undecided)

1. Confirm live package after bump (**current**; not 0.11.8).
2. Keep four roles co-located on one RocksDB vs split backends.
3. FTS internal vs external after real search UX.
4. Blob backend RocksDB vs filesystem vs S3.
5. Any of the above as a **written** acceptance.
6. New store evidence file after the package bump (this file stays historical).

## 8. How to re-verify

```bash
nix eval --raw .#nixosConfigurations.mail-vps.pkgs.stalwart-mail.version
nix eval --json .#nixosConfigurations.mail-vps.config.services.stalwart-mail.settings | jq .
nix build --no-link --print-out-paths .#nixosConfigurations.mail-vps.pkgs.stalwart-mail.src
# then rg in that store path under crates/store
```
