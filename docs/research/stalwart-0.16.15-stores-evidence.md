# Evidence: Stalwart 0.16.15 storage and config (Surmount pin)

**Status:** research finding for the Surmount **engine** pin **0.16.15**.
Not operator acceptance of any layout.
**Store / packaging evidence date:** 2026-07-30.
**Host channel + module path refresh:** 2026-08-07 (living tree).

**Living host (2026-08-07):** flake input **`nixos-26.05`** @
`445d861c6d31b4af0c79d8d4be2331f762a361d7`. Sample `system.stateVersion =
"26.05"`. Surmount dual-disables both stock module paths
(`services/mail/stalwart-mail.nix` and `services/mail/stalwart.nix`) and
claims option path `services.stalwart` (not stock TOML body). Living pins:
[COMPACTION-PIN.md](../COMPACTION-PIN.md),
[architecture-review.md](../architecture-review.md),
[version-audit.md](version-audit.md).

**Scope:** Surmount engine pin (still **0.16.15** FODs as of living refresh),
how they are packaged, the 0.16 storage / config model from official docs and
tag `v0.16.15` source, what Surmount modules actually write, and what remains
unverified in the release binary. Store-shape measurements are **2026-07-30**
evidence and still apply to engine **0.16.15** unless a later engine bump
lands.

**Historical only (do not treat as current):**
[stalwart-stores-evidence-2026-07-30.md](stalwart-stores-evidence-2026-07-30.md)
documents the old nixpkgs **0.11.8** TOML era (host was on 25.05 then).

**Packaging join (not architecture law):**
`.grok/joins/stalwart-current.md`

---

## 1. Exact versions in the Surmount tree

Engine package versions measured 2026-07-30 via package files, `nix build`,
and `--version`. Living tree still pins these FODs (re-check
[COMPACTION-PIN.md](../COMPACTION-PIN.md) / `nix/packages/` before claiming a
bump).

| Component | Version | Packaging | Upstream assets | Evidence |
|-----------|---------|-----------|-----------------|----------|
| Stalwart server | **0.16.15** | Release binary FOD | `stalwart-{x86_64,aarch64}-unknown-linux-gnu.tar.gz` from `stalwartlabs/stalwart` tag `v0.16.15` | `nix/packages/stalwart-mail.nix`; `stalwart --version` -> `0.16.15` |
| stalwart-cli | **1.0.12** | Release binary FOD | `stalwart-cli-*-unknown-linux-gnu.tar.xz` from `stalwartlabs/cli` | `nix/packages/stalwart-cli.nix`; `stalwart-cli --version` -> `stalwart-cli 1.0.12` |
| WebUI assets | **1.0.7** | Release zip FOD | `webui.zip` from `stalwartlabs/webui` | `nix/packages/stalwart-webui.nix` |
| spam-filter rules | **3.0.0** | Release toml + rules FODs | `spam-filter.toml`, `spam-filter-rules.json.gz` from `stalwartlabs/spam-filter` | `nix/packages/stalwart-spam-filter.nix` |
| Packaging mode | `release-binary-fod` | Not source build | `passthru.packagingMode` | `nix eval --raw .#stalwart-mail.passthru.packagingMode` |

### Realized store paths (this machine, after `nix build`)

| Output | Store path (example) | Approx closure size |
|--------|----------------------|---------------------|
| `stalwart-mail` | `/nix/store/yk1vn9sqy3n9m5spicxxyw7nb4b1fknn-stalwart-mail-0.16.15` | ~126.5 MiB |
| `stalwart-cli` | `/nix/store/i3g5qb53y7iibya3c891738g9pg2cgi4-stalwart-cli-1.0.12` | ~39.0 MiB |
| `stalwart-webui` | `/nix/store/67khijzlqxxmkfqnp4fmzaq138mmpfkn-stalwart-webui-1.0.7` | (zip FOD) |
| `stalwart-spam-filter` | `/nix/store/gk4z0c861yymycf6dbpf4a44c2nbvbf2-stalwart-spam-filter-3.0.0` | (rules FODs) |

### Binary shape (server)

- Main program name: `stalwart` (compat symlink `stalwart-mail` -> `stalwart`)
- ELF 64-bit pie, dynamically linked, **stripped**
- Linked mainly against glibc (`libm`, `libpthread`, `libc`, `libdl`); OpenSSL
  may be pulled via patchelf inputs depending on build
- Release tarball binary size on disk: about 99 MB before install layout

### Why FOD binaries (not rustPlatform)

From package comments and join (reasons as of 2026-07-30 packaging day):

1. `fetch-cargo-vendor` hit crates.io **HTTP 403** for some crates in this
   environment (2026-07-30).
2. At packaging time the host was still on nixos-25.05; CLI source build
   wanted rustc newer than that channel's default 1.86 for some unstable lib
   features. Living host is **nixos-26.05** / rustc **1.95**; source build is
   still deferred for the vendor-403 path, not because the host is on 25.05.
3. Release binaries are hermetic fixed-output hashes and match the published
   tag. Source build remains a future path, not current.

### Upstream source tag (for crate inventory)

- Repo: https://github.com/stalwartlabs/stalwart
- Tag: **v0.16.15**
- Workspace crate `store` version: **0.16.15**
  (`crates/store/Cargo.toml`)
- Workspace crate `directory` version: **0.16.15**
- Binary package `stalwart` version: **0.16.15**
  (`crates/main/Cargo.toml`)

### Third-party store-related crates (from tag `Cargo.lock`)

| Crate | Locked version | Role |
|-------|----------------|------|
| `rocksdb` | **0.24.0** | RocksDB bindings (`multi-threaded-cf`) |
| `librocksdb-sys` | **0.17.3+10.4.2** | Bundled RocksDB C++ **10.4.2** in lock |
| `blake3` | **1.8.5** | Blob content hash |
| `lz4_flex` | **0.13.1** (also 0.10.0 present) | App-level blob compression |
| `roaring` | **0.11.4** | Bitmap sets (search / query) |
| `bitpacking` | **0.9.3** | Search postings packing |
| `rusqlite` | **0.40.1** | SQLite backend (feature) |
| `tokio-postgres` | **0.7.18** | PostgreSQL backend (feature) |
| `mysql_async` | **0.36.2** | MySQL backend (feature) |
| `foundationdb` | **0.11.0** | FoundationDB backend (feature) |
| `redis` | **1.4.1** | In-memory backend (feature) |
| `rust-s3` | (feature `s3`, version from store Cargo.toml **0.37**) | S3 blobs |
| Azure crates | `azure_*` **0.21.0** (feature) | Azure blob backend |

Elasticsearch / Meilisearch clients are implemented in-tree under
`crates/store/src/backend/elastic` and `.../meili` using **reqwest** (no
separate `elasticsearch` crate line found in the lock search for that name).
They are **not** gated by a Cargo feature on the `SearchStore` enum; they
compile whenever the store crate is built.

### Release binary feature set (upstream Dockerfile, not our Nix)

Upstream `Dockerfile` at tag v0.16.15 builds with:

```text
--no-default-features --features "sqlite postgres mysql rocks s3 redis azure nats enterprise"
```

Workspace `crates/main` default features are only `"rocks", "enterprise"`;
the **published** GNU tarball is a multi-backend build closer to the Docker
feature set. Binary string evidence on Surmount's FOD includes symbols /
type names for RocksDB, SQLite, PostgreSQL, MySQL, FoundationDB, S3/Azure,
Redis cluster, ElasticSearch, Meilisearch, and blake3 (see section 4).

---

## 2. Host nixpkgs channel vs Surmount engine

**Living truth (2026-08-07):** host channel is **`nixos-26.05`**. The mail
**engine is not** taken from the host channel; it remains Surmount FOD pin
**0.16.15**.

| Item | Living value (2026-08-07) | Where |
|------|---------------------------|--------|
| flake input | `github:NixOS/nixpkgs/nixos-26.05` | `flake.nix` |
| locked rev | `445d861c6d31b4af0c79d8d4be2331f762a361d7` | `flake.lock` node `nixpkgs` |
| Sample `system.stateVersion` | **26.05** | `hosts/mail-vps/configuration.nix`, tests |
| Channel stock package | nixos-26.05 ships `pkgs.stalwart` (lagged vs Surmount FOD; not used) | nixpkgs; see [version-audit.md](version-audit.md) |
| Surmount engine | overlay / flake package **0.16.15** | `nix/packages/stalwart-mail.nix`, `flake.nix` |
| Stock modules | dual `disabledModules`: `services/mail/stalwart-mail.nix` + `services/mail/stalwart.nix` | `modules/stalwart-service.nix` |
| Option path | `services.stalwart` (matches stock attr; Surmount 0.16 config.json module) | same module |

### Historical footnote: host on 2026-07-30 store-evidence day

When store packaging evidence was gathered, the host was still **nixos-25.05**:

| Item | Value (2026-07-30 only) |
|------|-------------------------|
| flake input | `github:NixOS/nixpkgs/nixos-25.05` |
| locked rev | `ac62194c3917d5f474c1a844b6fd6da2db95077d` |
| `pkgs.lib.version` (mail-vps eval then) | `25.05pre-git` |
| Channel package `pkgs.stalwart-mail` | **0.11.8** on 25.05 (not Surmount engine) |
| Module disable (then) | single path `services/mail/stalwart-mail.nix` was enough on 25.05 |

Do **not** claim Surmount still boots 25.05. Engine store evidence below
still targets pin **0.16.15**.

---

## 3. Storage model (0.16 generation) + what Surmount configures

### 3.1 Four store roles still exist

Official docs still describe **four independent store roles**:

| Role | Object name (0.16) | Holds (docs) |
|------|--------------------|--------------|
| Data | `DataStore` | Structured metadata: mailbox state, folders, settings, domains, calendars/contacts metadata, most config objects |
| Blob | `BlobStore` | Large binaries: messages, attachments, Sieve scripts |
| Search / FTS | `SearchStore` | Full-text indexes |
| In-memory | `InMemoryStore` | Ephemeral KV: rate limits, locks, OAuth codes, ACME tokens, greylist, etc. |

Sources: https://stalw.art/docs/storage/ ,
https://stalw.art/docs/install/store/

In source (`crates/store/src/lib.rs` at v0.16.15) the runtime still has
typed enums: `Store`, `BlobStore`, `SearchStore`, `InMemoryStore`.

### 3.2 Config model change (the big 0.16 break)

From `UPGRADING/v0_16.md` at tag v0.16.15 and Surmount modules:

1. **No TOML.** On-disk config is a small **`config.json`** that describes
   **only the DataStore** (tagged JSON object with `@type`).
2. **Everything else** (listeners, accounts, spam, TLS, BlobStore /
   SearchStore / InMemoryStore choices, WebUI URL, etc.) lives **inside the
   datastore as JMAP objects**.
3. Day-2 admin: **WebUI** or **`stalwart-cli apply`** (declarative plan),
   plus `get` / `update` / `query` on named objects.
4. Old REST management API is gone; management is JMAP at `/jmap`.
5. Account names must be email addresses in 0.16 (upgrade path documents
   bare-name migration). Surmount is greenfield, so this is mainly a
   constraint for future account creation, not a migration task.

Declarative ops note from upstream upgrade doc: keep managing `config.json`
with Nix/Ansible; manage the rest with `stalwart-cli apply` plans.

### 3.3 Backend matrix (documented)

From https://stalw.art/docs/install/store/ and
https://stalw.art/docs/storage/backends/ :

| Backend | Data | Blob | Search | In-memory |
|---------|------|------|--------|-----------|
| RocksDB | yes | yes | yes | yes |
| FoundationDB | yes | yes | yes | yes |
| PostgreSQL | yes | yes | yes | yes |
| MySQL / MariaDB | yes | yes | yes | yes |
| SQLite | yes | yes | yes | yes |
| S3 / MinIO | | yes | | |
| Azure Blob | | yes | | |
| Filesystem | | yes | | |
| ElasticSearch | | | yes | |
| Meilisearch | | | yes | |
| Redis / Valkey | | | | yes |

**Single-node recommendation (docs):** RocksDB for all four roles is a normal
lightweight layout. RocksDB is called out as recommended for single-node data
store. Distributed sketch: FoundationDB + S3 + Redis + ES/Meilisearch.

**Backend change cost:** docs warn that changing backends later requires
**data migration**.

### 3.4 DataStore JSON variants (on-disk `config.json`)

Reference: https://stalw.art/docs/ref/object/data-store

Surmount-relevant RocksDB fields:

| Field | Docs default | Meaning |
|-------|--------------|---------|
| `@type` | (required) | `"RocksDb"` (Pascal case in JSON) |
| `path` | required | RocksDB directory |
| `blobSize` | **16834** | Min size (bytes) to spill into blob store vs inline metadata; max 1048576, min 1024 |
| `bufferSize` | **134217728** (128 MiB) | Write buffer size |
| `poolWorkers` | CPU count | Worker threads |

Other `@type` values on DataStore: `Sqlite`, `FoundationDb`, `PostgreSql`,
`MySql` (field sets differ; SQL needs host/auth, etc.).

### 3.5 Blob store (JMAP object, not config.json)

- Variants include: `Default` (reuse data store), `S3`, `Azure`,
  `FileSystem`, `FoundationDb`, `PostgreSql`, `MySql`, `Sharded` (enterprise).
- **Content addressing:** official blob docs say every blob is addressed by
  **BLAKE3** of the bytes (dedup across users/mailboxes).
- Source confirmation (`crates/types/src/blob_hash.rs` at v0.16.15):
  - `BLOB_HASH_LEN = 32`
  - `BlobHash::generate` -> `blake3::hash(value).into()`
- **App-level compression:** Email object field `compressionAlgorithm`;
  default **LZ4** (`"lz4"`), or `"none"`. Separate from RocksDB blob-file
  machinery.

### 3.6 Search store

- Variants: `Default` (internal index in data store), `ElasticSearch`,
  `Meilisearch`, plus dedicated FoundationDB / PostgreSQL / MySQL variants
  on the SearchStore object.
- Docs for internal / Default: indexes stored using **bloom filters**
  (probabilistic membership; multi-language via NLP). Also note internal FTS
  has higher write amplification vs external engines.
- Source still uses `roaring` + `bitpacking` crates (same family as 0.11
  internal postings). Docs emphasize bloom-filter wording for privacy/
  structure; code also has roaring/bitpacking. Prefer citing both until a
  deeper FTS walk is done.
- Search indexing scope is controlled by a separate **Search** object
  (`indexEmail`, contacts, calendar, telemetry, languages).

### 3.7 In-memory store

- Variants: `Default` (data store), `Redis`, `RedisCluster`, `Sharded`
  (enterprise), plus HTTP/static style backends in code
  (`Http`, `Static` on `InMemoryStore` enum).
- Soft state; Redis recommended for heavy/distributed write load.
- When Default uses durable RocksDB/SQL, expired keys need data-store cleanup
  schedule (`DataRetention.dataCleanupSchedule`).

### 3.8 RocksDB implementation notes (source)

File: `crates/store/src/backend/rocksdb/main.rs` (v0.16.15)

- Opens `OptimisticTransactionDB` with multi-threaded column families
- Blobs CF: `set_enable_blob_files(true)`,
  `set_min_blob_size(config.blob_size)`, blob GC enabled
- DB opts: `set_write_buffer_size(config.buffer_size)`
- Config field names in **JSON/docs**: `blobSize`, `bufferSize` (camelCase)
- Internal Rust fields map from those (source uses `config.blob_size` /
  `config.buffer_size` after deserialize)

Subspace byte tags still exist (partial list from `lib.rs`):

| Const | byte | Role |
|-------|------|------|
| `SUBSPACE_DIRECTORY` | `d` | directory rows |
| `SUBSPACE_PROPERTY` | `p` | properties |
| `SUBSPACE_REGISTRY` | `s` | settings / registry |
| `SUBSPACE_BLOBS` | `t` | blobs CF |
| `SUBSPACE_SEARCH_INDEX` | `z` | search index |
| `SUBSPACE_IN_MEMORY_VALUE` | `m` | durable lookup values |
| `SUBSPACE_QUEUE_*` | `e` / `q` | queues |
| ... | | see `lib.rs` |

Note: FTS subspace letter moved vs 0.11 (`g` was FTS; in 0.16 `g` is
`SUBSPACE_REGISTRY_PK` and search is `z`). Do not reuse 0.11 subspace maps
for ops on 0.16.

### 3.9 Directory (not a fifth storage role)

Official: https://stalw.art/docs/install/directory/

| Backend | Notes |
|---------|--------|
| **Internal** | Account data in the **data store**. Full admin in WebUI/CLI. Default when Authentication.directoryId unset. |
| LDAP | External auth |
| SQL | External (PostgreSQL / MySQL / SQLite) |
| OpenID Connect | External (Keycloak, Authentik, etc.) |

Directory crate modules at v0.16.15: `backend/{ldap,oidc,sql}`, plus internal
via store. Surmount day-one intent remains **internal directory** (provisional;
see `docs/DATASTORES.md`).

### 3.10 What Surmount actually configures

#### Generated `config.json` (mail-vps eval)

```json
{"@type":"RocksDb","path":"/var/lib/stalwart-mail/db"}
```

Measured path example:
`/nix/store/57x78pmmlqsbrrs92v7xqinwrsi3sv8l-stalwart-config.json`

`blobSize` / `bufferSize` are **omitted** so upstream defaults apply
(16834 / 134217728 per docs and module comments).

#### Module options (`modules/stalwart-service.nix`)

| Option | mail-vps / Surmount value |
|--------|---------------------------|
| `enable` | true (via `modules/mail.nix`) |
| `package` | flake `stalwart-mail` 0.16.15 |
| `dataDir` | `/var/lib/stalwart-mail` (`surmount.mailDataDir`) |
| `storeType` | `"RocksDb"` |
| `storePath` | `/var/lib/stalwart-mail/db` |
| `blobSize` / `bufferSize` | null (defaults) |
| `configFile` | null (generated) |
| `openFirewall` | false |
| `settings` | accepted and **ignored** (no TOML writer) |
| Unit | `stalwart-mail.service` |
| ExecStart | `stalwart --config=<config.json>` |
| User/group | `stalwart-mail` |
| Caps | `CAP_NET_BIND_SERVICE` |

#### Hermetic FODs on host (not auto-consumed by engine)

Under `/etc/surmount/stalwart/` when mail module is on:

- `spam-filter.toml`
- `spam-filter-rules.json.gz`
- `webui.zip`

0.16 first boot may still pull WebUI / spam rules from GitHub until the
operator points Application / SpamSettings at `file://` paths via WebUI or
`stalwart-cli apply`. Known gap in packaging join.

#### First-boot listeners (upstream defaults, not Nix-declared)

Empty RocksDB + safe defaults (per join / mail.nix comments): SMTP :25,
submissions :465, IMAPS :993, ManageSieve :4190, HTTP **:8080**, HTTPS :443,
POP3S :995. Plain :587 / :143 are **not** default in 0.16 (PACC / upgrade
notes). Management UI default port on Surmount moved **8080 -> 8090** to
avoid colliding with Stalwart HTTP.

#### Roles co-location

Surmount writes **one RocksDB path** as DataStore. BlobStore / SearchStore /
InMemoryStore are **not** declared in Nix; after first boot they follow
upstream **Default** (same data store) unless changed via apply/WebUI.
That is provisional Surmount wiring, not a locked product decision.

---

## 4. UNKNOWN / not verified in the binary

Things this research did **not** fully prove against the stripped FOD:

1. **Exact Cargo features of the published GNU tarball** vs Dockerfile line.
   Binary strings strongly suggest multi-backend; we did not rebuild from
   source or dump a build manifest from the tarball.
2. **Whether FoundationDB is actually usable** in the FOD (type name present;
   feature may still need cluster libraries at runtime).
3. **Runtime default objects** after first boot (exact BlobStore / SearchStore
   / InMemoryStore singleton JSON). Inferred from docs + empty-store
   defaults, not captured from a live `stalwart-cli get` on a booted VM.
4. **Internal FTS structure detail** beyond docs (bloom) + crates
   (roaring/bitpacking). No red/green test of search behavior on this pin.
5. **RocksDB compression codec** at the CF level (0.11 had explicit
   `compression = "lz4"` in TOML; 0.16 RocksDB JSON fields documented are
   path / blobSize / bufferSize / poolWorkers only). App-level blob
   compression is on the Email object (default LZ4).
6. **Import path** for Maildir on CLI 1.0.12: live `--help` has **no**
   `import` subcommand (2026-08-13). Path is Vandelay 1.0.7 + wrapper.
7. **Full `mail-vm-test`** not run for this evidence pass.
8. **nixpkgs-unstable** lag note (0.16.14 / incompatible module) was from the
   packaging join; not re-fetched against current unstable in this pass.

---

## 5. Diffs vs historical 0.11.8 note

Point at: [stalwart-stores-evidence-2026-07-30.md](stalwart-stores-evidence-2026-07-30.md).
That file is **historical only**.

| Topic | 0.11.8 (historical scaffold) | 0.16.15 (current Surmount pin) |
|-------|------------------------------|--------------------------------|
| Package source | nixpkgs 25.05 `buildRustPackage` | Surmount release-binary FOD |
| Semver | 0.11.8 | **0.16.15** |
| Config on disk | TOML `store.*` / `storage.*` | **config.json** DataStore only |
| Role selection | `storage.data/blob/fts/lookup = "db"` | JMAP singletons DataStore / BlobStore / SearchStore / InMemoryStore |
| Admin API | REST + old CLI | JMAP + **stalwart-cli** 1.x apply/get/update |
| WebUI | older webadmin packaging | separate **webui 1.0.7** zip / Application.resource_url |
| Binary name | `stalwart-mail` | `stalwart` (+ compat symlink) |
| Meilisearch | not in 0.11 store features | **yes** (docs + backend + binary string) |
| Blob hash | blake3, 32-byte | **still blake3**, locked **1.8.5** |
| RocksDB rust crate | 0.23.0 | **0.24.0** |
| librocksdb-sys | 0.17.1+9.9.3 (nixpkgs often linked system 10.2.1) | lock **0.17.3+10.4.2** (bundled in upstream build) |
| RocksDB option names | TOML `min-blob-size`, `write-buffer-size` | JSON **`blobSize`**, **`bufferSize`** |
| Default single-node | one rocksdb id `db` for all roles | one RocksDB DataStore; other roles Default -> same store |
| FTS subspace | `g` in 0.11 map | search index subspace **`z`** in 0.16 source |
| Directory config | TOML `directory.internal` | Authentication.directoryId unset -> internal on DataStore |
| Upgrade story | n/a for greenfield | user data layout largely kept across 0.15->0.16; **config/management rewritten** (see UPGRADING) |

---

## 6. Sources

### Official docs (current product docs site)

- Storage overview: https://stalw.art/docs/storage/
- Choosing a database: https://stalw.art/docs/install/store/
- Storage backends: https://stalw.art/docs/storage/backends/
- Data store: https://stalw.art/docs/storage/data
- Blob store: https://stalw.art/docs/storage/blob
- Search store: https://stalw.art/docs/storage/fts
- In-memory store: https://stalw.art/docs/storage/in-memory
- RocksDB backend: https://stalw.art/docs/storage/backends/rocksdb
- DataStore object ref: https://stalw.art/docs/ref/object/data-store
- BlobStore object ref: https://stalw.art/docs/ref/object/blob-store
- SearchStore object ref: https://stalw.art/docs/ref/object/search-store
- Directory choose: https://stalw.art/docs/install/directory/
- Internal directory: https://stalw.art/docs/auth/backend/internal/

### Upstream source / upgrade (tag v0.16.15)

- Repo: https://github.com/stalwartlabs/stalwart
- Upgrade notes: https://github.com/stalwartlabs/stalwart/blob/v0.16.15/UPGRADING/v0_16.md
  (also raw `UPGRADING/v0_16.md`)
- `crates/store/Cargo.toml`, `crates/store/src/lib.rs`
- `crates/store/src/backend/rocksdb/main.rs`
- `crates/types/src/blob_hash.rs` (blake3 generate)
- `crates/main/Cargo.toml`, root `Dockerfile` (release feature list)
- `Cargo.lock` (crate semvers above)
- CLI repo: https://github.com/stalwartlabs/cli (v1.0.12)
- WebUI: https://github.com/stalwartlabs/webui (v1.0.7)
- spam-filter: https://github.com/stalwartlabs/spam-filter (v3.0.0)

### Surmount tree paths

| Path | Role |
|------|------|
| `nix/packages/stalwart-mail.nix` | Server 0.16.15 FOD |
| `nix/packages/stalwart-cli.nix` | CLI 1.0.12 FOD |
| `nix/packages/stalwart-webui.nix` | WebUI 1.0.7 FOD |
| `nix/packages/stalwart-spam-filter.nix` | spam-filter 3.0.0 FODs |
| `modules/stalwart-service.nix` | config.json + systemd unit |
| `modules/mail.nix` | Surmount wiring, FODs under `/etc/surmount/stalwart/` |
| `flake.nix` / `flake.lock` | packages, overlay; living host nixos-26.05 lock |
| `.grok/joins/stalwart-current.md` | packaging join |
| `docs/DATASTORES.md` | living datastore inventory (may lag wording) |
| `docs/research/stalwart-stores-evidence-2026-07-30.md` | historical 0.11.8 only |

### Commands used for measurement

```bash
nix build .#stalwart-mail .#stalwart-cli .#stalwart-webui .#stalwart-spam-filter --no-link --print-out-paths
# then:
#   result/bin/stalwart --version          # 0.16.15
#   result/bin/stalwart-cli --version      # stalwart-cli 1.0.12
nix eval --raw .#stalwart-mail.version
nix eval --raw .#stalwart-mail.passthru.packagingMode
nix eval --raw .#nixosConfigurations.mail-vps.config.services.stalwart.storePath
# ExecStart embeds generated config.json; cat that store path for JSON body
jq -r '.nodes.nixpkgs.locked.rev' flake.lock
```

---

## 7. How to re-verify after a bump

1. Bump versions/hashes per `.grok/joins/stalwart-current.md`.
2. `nix build .#stalwart-mail && result/bin/stalwart --version`
3. Skim `UPGRADING/` in the new tag for config.json / apply changes.
4. Re-read https://stalw.art/docs/storage/ and DataStore object ref.
5. Optionally fetch tag `Cargo.lock` for crate bumps (rocksdb, blake3).
6. Update **this file** (or add a dated sibling) rather than editing the
   historical 0.11.8 note in place.
