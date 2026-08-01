# Surmount data plane (datastores)

Intentional design of every durable and semi-durable store in the Surmount
mail + web stack. Written for a senior database engineer: precise, no
"defaults are fine," explicit unknowns, load-test honesty.

**Last updated:** 2026-07-30

> **Operator direction 2026-07-30:** RocksDB for all roles is **fine for now**;
> co-location OK this phase; internal FTS good enough for now; in-memory on
> RocksDB fine; blob tiering/compliance split not now; clever RPO/RTO later.
> See [operator-direction.md](operator-direction.md). This file remains the
> deep inventory and upstream mechanics note.

> Four-store mechanics are **upstream engine facts**. Current pin evidence:
> [research/stalwart-0.16.15-stores-evidence.md](research/stalwart-0.16.15-stores-evidence.md).
> Historical 0.11.8 only:
> [research/stalwart-stores-evidence-2026-07-30.md](research/stalwart-stores-evidence-2026-07-30.md).

> **Engine version:** Surmount pins Stalwart **0.16.15** via release-binary FODs
> (`nix/packages/stalwart-mail.nix`, `modules/stalwart-service.nix`). Historical
> 0.11.8 was a nixos-25.05 scaffold accident, not a product choice.

**Design notes:** [open-choices.md](open-choices.md),
[operator-direction.md](operator-direction.md).
**Companions:** [STACK.md](STACK.md), [OPS.md](OPS.md),
[MIGRATION.md](MIGRATION.md), [SECRETS.md](SECRETS.md),
[SECURITY.md](SECURITY.md), [glossary.md](glossary.md).

Official Stalwart storage docs (authoritative for engine behavior):

- [Choosing a database](https://stalw.art/docs/install/store/)
- [Storage overview](https://stalw.art/docs/storage/)
- [Data store](https://stalw.art/docs/storage/data)
- [Blob store](https://stalw.art/docs/storage/blob)
- [Search / FTS](https://stalw.art/docs/storage/fts/)
- [In-memory store](https://stalw.art/docs/storage/in-memory)
- [Backends](https://stalw.art/docs/storage/backends/)
- [RocksDB](https://stalw.art/docs/storage/backends/rocksdb/)
- [Choosing a directory](https://stalw.art/docs/install/directory/)
- [Internal directory](https://stalw.art/docs/auth/backend/internal/)

---

## 1. Design posture

1. **Every store is a choice.** nixpkgs defaults are a *proposal*, not
   architecture. We either adopt them with written rationale and acceptance
   criteria, or reject them with an alternative.
2. **Logical model first, physical backend second.** Stalwart's four-store
   roles are the working model. Co-locating them in one RocksDB is **fine for
   now** (operator direction 2026-07-30); not a forever lock.
3. **We have not load-tested** this host under production mailbox counts,
   concurrent IMAP/JMAP, or large Maildir import. Numbers below are
   design intent and upstream guidance, not Surmount SLOs. Host size
   direction is ~16 GB RAM / 2 TB NVMe / 16 cores (operator-direction.md).
4. **Do not read engine files from product code.** Surmount talks to
   Stalwart over HTTP/JMAP/admin/CLI. Direct RocksDB/SQL access from
   `surmount-management-ui` is out of bounds.
5. **MailPlus Maildir is import source only.** Runtime store is Stalwart's
   data+blob (+FTS). See [MIGRATION.md](MIGRATION.md).

---

## 2. Stalwart logical model

### 2.1 Four store roles

| Role | Official name | Holds | Consistency expectations |
|------|---------------|-------|--------------------------|
| **Data** | Data store | Structured metadata: mailbox state, folders, headers/refs, account settings, domains, calendars/contacts metadata, most config objects | Source of truth for "what exists." Must stay consistent with blob refs (orphan blobs / dangling refs are failure modes cleaned by scheduled tasks). |
| **Blob** | Blob store | Large binaries: raw messages, attachments, Sieve scripts | Content-addressed (BLAKE3 hash of bytes). Identical content shares one physical object. Small objects may stay **inline** in the data store below a size threshold. |
| **Search** | Search / FTS store | Full-text indexes over bodies, attachments, contacts, calendar (as configured) | Eventually consistent with data+blob. Rebuild/reindex possible; stale index means wrong/missing search hits, not silent mail loss. |
| **In-memory / lookup** | In-memory store | Ephemeral KV: rate limits, greylist tokens, locks, OAuth codes, ACME tokens (if Stalwart ACME used), Sieve auto-responder ids | Soft state. Loss on restart is usually acceptable (limits reset, greylist cold). When backed by durable data store, expired keys need periodic purge. |

Sources: [install/store](https://stalw.art/docs/install/store/),
[storage overview](https://stalw.art/docs/storage/).

**Single-node consolidation:** Stalwart documents that RocksDB or PostgreSQL
can serve all four roles. Distributed deployments typically split:
FoundationDB (data) + S3 (blob) + Redis (in-memory) + ES/Meilisearch
(search). Surmount is single-node for now (multi-host deferred).

**Backend change cost:** switching backends after go-live requires **data
migration**. Choose deliberately at first durable write.

### 2.2 Directory vs mail data

| Concept | What it is | Where it lives (our default) |
|---------|------------|------------------------------|
| **Directory** | Authn/authz backend: principals, credentials, quotas, group membership, address lookup | **Internal directory** stored **in the data store** (`directory.internal` -> store `db`) |
| **Mail data** | Messages, folders, flags, blob pointers, FTS | Data + blob + FTS stores |

Internal directory:
[docs](https://stalw.art/docs/auth/backend/internal/). Accounts are managed
via WebUI, JMAP management APIs, or `stalwart-cli`. External directories
(LDAP, SQL, OIDC) are supported by Stalwart but **not** Surmount day-one;
product identity is Nostr at the Axum layer (proposed), bridged to Stalwart
mail credentials.

**Important:** directory and data store should stay on **compatible**
backends. Mixing (e.g. RocksDB directory + Postgres data) has bitten
operators upstream; prefer one data plane for internal directory + data.

### 2.3 Message keying and blob vs metadata split

Conceptual model (engine-owned; Surmount must not reimplement):

```text
  Account / mailbox hierarchy  -->  data store (folders, UIDs, flags, ...)
           |
           +-- message metadata (headers summary, sizes, blob id(s))
           |
           v
  Blob id = BLAKE3(content)    -->  blob store (raw RFC822 / parts)
           |
           +-- optional inline in data store if size < blobSize threshold
           |
           v
  FTS tokens / bloom structures --> search store (internal default)
```

Stalwart RocksDB docs expose **`blobSize`**: minimum size (bytes) for an
object to spill into the blob store rather than inline metadata. Official
default documented as **`16834`** (engine constant; treat as upstream SoT,
not something we invent). Larger messages/attachments go to blob backend.

Blob compression is separate from RocksDB compression: Email object
`compressionAlgorithm` defaults to **LZ4** for blob payload bytes
([blob store](https://stalw.art/docs/storage/blob)).

### 2.4 FTS: internal vs external

| Mode | Backend | Characteristics (upstream) |
|------|---------|----------------------------|
| **Internal (Default)** | Indexes in the configured data store using **bloom-filter style** structures | No extra process; multi-language detection; privacy-friendly token obscurity (not encryption); **higher write amplification** than dedicated engines |
| Elasticsearch / OpenSearch | External HTTP | Strong analysis/ranking; memory-heavy; JVM ops |
| Meilisearch | External HTTP | Fast relevance UX; **maxTotalHits** default 1000 can silently truncate IMAP/JMAP "all matches"; nested boolean query mapping incomplete |
| PostgreSQL as FTS | PG `tsvector` | **~650 KB body + ~650 KB attachments** truncated per message before index |
| MySQL as FTS | MySQL FULLTEXT | No stemming; no multi-language same column; short words often dropped |

Sources: [FTS](https://stalw.art/docs/storage/fts/), backend pages.

**Rebuild / staleness:**

- FTS is derived. Losing only the search store should be recoverable by
  reindex (exact CLI/admin path is version-dependent; verify on installed
  `stalwart-cli` / WebUI before needing it).
- Losing data or blob is **not** recoverable from FTS alone.
- Encryption-at-rest (if enabled): only headers in FTS
  ([FTS note](https://stalw.art/docs/storage/fts/)).
- **We have not measured** internal FTS latency or write amp under Surmount
  mailbox sizes. Gate external FTS on observed quality/latency, not fashion.

### 2.5 Maintenance schedules (engine)

Not per-backend knobs in nixpkgs; controlled via Stalwart
[DataRetention](https://stalw.art/docs/storage/) object:

| Task | Purpose | Upstream default (docs) |
|------|---------|-------------------------|
| `expungeSchedule` | Auto-expunge | cron on object |
| `dataCleanupSchedule` | Data store cleanup (incl. expired in-memory keys when durable backend used) | daily ~02:00 |
| `blobCleanupSchedule` | Unreferenced / TTL blob GC | daily ~04:00 |

Enterprise undelete/archive is **not** Community Edition; do not plan on it
without licensing.

---

## 3. Physical backends (DB-engineer comparison)

When Surmount would choose each backend. All "Surmount" notes assume single
NixOS VPS unless stated.

### 3.1 RocksDB (embedded LSM)

| Dimension | Assessment |
|-----------|------------|
| **Process model** | **In-process** library inside `stalwart-mail`. No separate DB daemon. Single writer process assumptions (one Stalwart instance owns the DB directory). |
| **Durability** | LSM + WAL. Survives process crash if fsync policy honored by RocksDB/OS. Unclean shutdown: recovery from WAL on open (standard RocksDB). **We have not audited** Stalwart's RocksDB `Options` (WAL sync mode, `atomic_flush`, etc.) beyond what upstream exposes. |
| **Backup/restore** | Practical single-node story: **stop service** (or freeze FS) then restic/snapshot of `dataDir` (especially `db/`). Live restic of open RocksDB can capture torn state unless using filesystem freeze, LVM/ZFS snapshot of consistent volume, or RocksDB checkpoint API (**Stalwart does not clearly document** a first-class online checkpoint export for operators; treat hot copy of `db/` as **risky**). Restore = replace files + start service. |
| **Online ops** | Compaction is internal. Disk may grow after deletes until compaction/GC; Stalwart blob/data cleanup schedules matter. No `VACUUM` SQL. Monitor disk, not just mailbox quota. |
| **Concurrency / multi-node** | **Single node.** Do not point two Stalwart processes at one RocksDB path. Multi-node needs different backends (deferred). |
| **Ops burden (1 VPS)** | Lowest: no Postgres/Redis/ES to patch. Cost is LSM literacy (write amp, space amp, "why is disk still full"). |
| **Failure modes** | Disk full mid-compaction; corrupted SST after bad hardware; split-brain if two writers; backup of live files restores garbage; OOM if write buffer + caches oversized for RAM. |
| **When Surmount chooses it** | **Fine for now** (operator direction 2026-07-30) for all four roles on the single operator-chosen VPS. Not a forever lock. See operator-direction.md for knobs on the ~16 GB / 16-core box. |

**Knobs Stalwart actually documents** for RocksDB
([rocksdb backend](https://stalw.art/docs/storage/backends/rocksdb/)):

| Field | Meaning | Upstream default | Surmount start (16G/16c) |
|-------|---------|------------------|-------------------------|
| `path` | DB directory | required | `/var/lib/stalwart-mail/db` |
| `blobSize` | Inline vs blob spill threshold (bytes) | `16834` | omit (default) until measured |
| `bufferSize` | In-memory write buffer (bytes) | `134217728` (128 MiB) | omit; optional 256 MiB if import write-bound |
| `poolWorkers` | DB worker threads | CPU count | omit (=16) or pin 8 to leave cores |

Surmount 0.16 module (`modules/stalwart-service.nix`) writes `config.json`
DataStore JSON. Optional Nix options: `blobSize`, `bufferSize` (null = omit).
`poolWorkers` not yet a first-class Nix option (Day-2 apply or add later).

**Compression:** app-level blob LZ4 is separate from RocksDB internal block
compression. 0.16 JSON surface for RocksDB is path + optional knobs above
(not the old 0.11 TOML world).

**Column families:** Stalwart does not document operator-facing CF layout.
Treat internal CF use as engine private.

**Co-location tradeoffs:**

| Layout | Pros | Cons |
|--------|------|------|
| **One RocksDB for all four roles** (OK for now) | One backup tree; one process; matches Stalwart single-node guidance; operator direction | Shared failure domain; FTS write amp hits same LSM; harder to put blobs on bigger/cheaper volume later without migration |
| Split RocksDB paths (multiple `store.*` entries) | Separate disks/quotas | Still single host; more paths to back up; little HA win |
| Data on Postgres + blob on fs/S3 + FTS internal/external | SQL ops familiarity; blob tiering | Migration cost; two+ systems; consistency windows |
| Full distributed (FDB + S3 + Redis + ES) | HA path | Wrong for deferred multi-host |

### 3.2 SQLite

| Dimension | Assessment |
|-----------|------------|
| **Process model** | Embedded (or multi-connection pool to one file). Stalwart: path + pool settings. |
| **Durability** | WAL mode typical in modern apps; confirm engine PRAGMAs via upstream. |
| **Backup** | File copy after stop, or `sqlite3 .backup`, or Litestream-style continuous. |
| **Online ops** | `VACUUM` / incremental vacuum; simpler mental model than LSM for many ops people. |
| **Concurrency** | Weaker writer concurrency than RocksDB/Postgres under heavy parallel SMTP+IMAP. |
| **Multi-node** | No shared-writer multi-node. |
| **Ops burden** | Low. |
| **Failure modes** | Single-file corruption; lock contention; huge single file. |
| **Surmount** | nixpkgs **legacy** path when `stateVersion < 24.11`: SQLite index + **filesystem blobs**. We are on **25.05** and **not** on that path. Revisit only with a written design update if RocksDB ops pain dominates and scale stays tiny. |

### 3.3 PostgreSQL

| Dimension | Assessment |
|-----------|------------|
| **Process model** | Separate daemon (`postgresql.service`). Network or socket. |
| **Durability** | Mature WAL, checkpoints, PITR with basebackup + WAL archive. |
| **Backup** | `pg_basebackup`, dump, or WAL-G/barman-style. Online backups well understood. |
| **Online ops** | Autovacuum, `REINDEX`, bloat monitoring, `EXPLAIN`. High operability if you already run PG. |
| **Concurrency** | Strong multi-connection. Read replicas: **Stalwart Enterprise** feature. |
| **Multi-node** | App multi-node possible with shared PG; still need blob/FTS strategy. |
| **Ops burden on 1 VPS** | Medium: another unit to patch, tune `shared_buffers`, disk for PG + mail. |
| **Failure modes** | Connection storms; bloat; bad vacuum; FTS size limits if PG used as search. |
| **FTS if used as search** | ~650 KB body/attachment truncation ([PG backend](https://stalw.art/docs/storage/backends/postgresql/)). |
| **Surmount would choose when** | (Open) Prefer SQL backup/PITR literacy; need ad-hoc SQL forensics; preparing multi-app shared PG (e.g. with Vaultwarden); RocksDB disk/ops pain past acceptance gates. **Not** day-one. |

### 3.4 MySQL / MariaDB

Similar to Postgres as networked RDBMS. Stalwart FTS limitations harsher
(no stemming, no multi-lang column, short-word drops).
**Surmount:** no preference over Postgres if an RDBMS is chosen; default
RDBMS candidate would be **PostgreSQL** for ecosystem fit with Vaultwarden
and NixOS module quality. MySQL only if external constraint.

### 3.5 FoundationDB

| Dimension | Assessment |
|-----------|------------|
| **Process model** | Separate cluster processes; client library on host. Special Stalwart build features. |
| **Durability / HA** | Designed for distributed ACID. |
| **Ops burden** | High for one VPS. |
| **Surmount** | **Not** until multi-host is no longer deferred. Upstream recommended for distributed Stalwart. |

### 3.6 S3 / MinIO / Azure blob (blob role only)

| Dimension | Assessment |
|-----------|------------|
| **Process model** | Network object store. |
| **Durability** | Provider-dependent. Stalwart default **`verifyAfterWrite = true`** (HEAD after PUT) because some S3-compat backends ACK before durable persist ([S3 docs](https://stalw.art/docs/storage/backends/s3/)). |
| **Backup** | Versioning + cross-region; or treat object store as primary and backup metadata store carefully. |
| **Surmount would choose when** | Blob growth outstrips VPS disk economics; or multi-node. Adds credentials, latency, partial-failure modes. Not day-one local disk. |

### 3.7 Filesystem blobs

Hashed directory tree (`path`, `depth` default 2). Simple single-node blob
tier; backup is file tree. Legacy nixpkgs paired this with SQLite.
**Surmount:** optional later split if we keep RocksDB/Postgres for data but
want blobs on a large mount; not current config.

### 3.8 Elasticsearch / Meilisearch (search only)

Extra daemons. Choose only after internal FTS fails acceptance (quality,
latency, write amp). Meilisearch: raise `maxTotalHits` for mail semantics;
accept nested-query approximation. ES: JVM memory tax on a small VPS is real.

### 3.9 Redis / Valkey (in-memory only)

| Dimension | Assessment |
|-----------|------------|
| **Role** | Rate limits, locks, short-lived tokens. **Cannot** be data/blob store. |
| **Persistence** | Usually unnecessary for Stalwart's ephemeral keys. |
| **Surmount** | Only if in-memory write load on RocksDB becomes hot or multi-node needs shared limits. Extra moving part on one VPS otherwise. |

---

## 4. RocksDB deep dive (if we keep it)

### 4.1 What we are adopting

- Embedded **LSM** key-value store, not a networked RDBMS.
- One process (`stalwart-mail`) opens `/var/lib/stalwart-mail/db`.
- lz4 compression at store config layer (nixpkgs).
- All four logical roles + internal directory reference store id `"db"`.

### 4.2 Single-writer assumptions

- Never run a second Stalwart (or any other process) with write access to the
  same RocksDB directory.
- Restore drills must stop the service before replacing `db/`.
- Backup agents that open RocksDB files while the service runs can capture torn state
  unless using a supported snapshot/checkpoint path (not documented as
  operator-facing in Stalwart docs we reviewed). Prefer:

  1. `systemctl stop stalwart-mail`
  2. restic/snapshot of `mailDataDir`
  3. `systemctl start stalwart-mail`

  Or volume-level atomic snapshot (ZFS/btrfs/LVM) of a quiet filesystem,
  still preferably with DB idle or brief freeze. **Document the method you
  actually use** in host runbooks after first production backup.

### 4.3 Unclean shutdown

Expect automatic WAL recovery on next open. If recovery fails: restore from
last good backup. Keep journald around the crash for support.

### 4.4 Growth and write amplification

- Internal FTS explicitly has **higher write amplification** than external
  FTS engines.
- Deletes/expunge free logical space; physical reclaim depends on compaction
  + blob cleanup schedule.
- Monitor: `du -sh /var/lib/stalwart-mail`, disk free, journal errors.

### 4.5 Tuning we can set vs mythical flags

**Settable via Stalwart settings (document if we override):**
`path`, `compression`, `blobSize`, `bufferSize`, `poolWorkers`.

**Not our job to twiddle without evidence:** raw RocksDB
`max_background_jobs`, `target_file_size_base`, manual `compact_range`, etc.
If upstream does not expose them, do not inject side-channel option files
without a design-doc update and upgrade test.

**RAM:** default write buffer 128 MiB plus block cache (engine-internal).
On the directed ~16 GB host, 128 MiB is a small slice; leave headroom for
SMTP bursts, spam-filter, edge, UI, and future Vaultwarden. Optional raise
to 256 MiB is documented in operator-direction.md. If OOM on a smaller box:
lower `bufferSize` only after measuring.

### 4.6 Co-locate vs split (decision aids)

Stay co-located while:

- Mailbox count and total corpus fit comfortably on one volume with room
  for compaction spikes (rule of thumb to revisit: **>50-80% disk** used by
  mail data plane, or restore time exceeds your RTO).
- Single operator prefers one backup path.
- No second Stalwart node.

Consider split when:

- Blob bytes dominate and cheaper object storage wins.
- You want PG PITR for metadata only.
- Internal FTS write amp or query quality fails gates (external search).
- Multi-host is no longer deferred.

---

## 5. Flake / nixpkgs reality (verified)

### 5.1 Module and stateVersion

- Option path (nixpkgs 25.05): `services.stalwart-mail`
- Source (locked rev `ac62194c...`): `nixos/modules/services/mail/stalwart-mail.nix`
- `useLegacyStorage = versionOlder stateVersion "24.11"`
- Host `system.stateVersion = "25.05"` => **non-legacy RocksDB path**

### 5.2 Exact store-related settings (flake eval)

Eval of `nixosConfigurations.mail-vps` on this tree (2026-07-30):

```toml
# Conceptual TOML equivalent of evaluated settings
[store.db]
type = "rocksdb"
path = "/var/lib/stalwart-mail/db"
compression = "lz4"

[storage]
data = "db"
fts = "db"
lookup = "db"
blob = "db"
directory = "internal"

[directory.internal]
type = "internal"
store = "db"
```

Also present (non-store but related):

- `spam-filter.resource` = **our FOD** `file://${spamFilterToml}` (v2.0.5),
  **not** the broken empty nixpkgs package path and **not** the message DB.
- `webadmin.path` = `/var/cache/stalwart-mail` (UI assets cache, not mail DB).
- Listeners, hostname, tracer stdout -> journald.

**Not set by us today (omit = upstream defaults):** `blobSize`, `bufferSize`,
`poolWorkers`, DataRetention crons, Email `compressionAlgorithm` (engine
default LZ4 for blobs). Starting-point rationale:
[operator-direction.md](operator-direction.md) section 2.

### 5.3 Paths and units

| Item | Value |
|------|-------|
| `surmount.mailDataDir` / `services.stalwart-mail.dataDir` | `/var/lib/stalwart-mail` |
| RocksDB directory | `/var/lib/stalwart-mail/db` |
| systemd unit | `stalwart-mail.service` |
| User | `stalwart-mail` (static user; avoids chown storms) |
| Import staging | `/var/lib/surmount/import/maildir` (Maildir **source**, not runtime) |

### 5.4 Spam-filter FOD is not the message store

`modules/mail.nix` pins
`https://github.com/stalwartlabs/spam-filter/releases/download/v2.0.5/spam-filter.toml`
as a fixed-output derivation so Stalwart does not download rules at runtime.
That TOML is **anti-spam configuration**, completely separate from RocksDB
message data. Do not back it up as if it were mail; it is in the Nix store
via the system closure.

### 5.5 Import path vs live store

```text
MailPlus Maildir  --operator rsync-->  /var/lib/surmount/import/...
                                            |
                          stalwart-cli import (operator-run)
                                            |
                                            v
                              Stalwart data + blob (+ FTS)
                              /var/lib/stalwart-mail/db
```

After successful import and verification, import staging is disposable.
Live authority is only the Stalwart store. Never auto-import on activation.

---

## 6. Surmount decision framework (storage)

### 6.1 Split acceptance

| Decision slice | Status |
|----------------|--------|
| Logical **four-store model** + internal directory in data store | **Working model** (upstream fact + proposed) |
| Physical **all-RocksDB co-location** on single VPS | **Scaffold default** (not operator-accepted) |
| External PG/S3/ES/Redis/FDB | **Not selected**; reopen with evidence + doc update |

### 6.2 Why provisional RocksDB co-location (rationale, not "nixpkgs said so")

1. Stalwart **recommends RocksDB for single-node** installations.
2. Matches nixpkgs 25.05 non-legacy defaults (we independently agree for
   now; we do not outsource judgment).
3. Minimizes process count on a mail VPS that already runs edge, UI, spam
   path, and later Vaultwarden.
4. One tree for restic paths simplifies first production backups.
5. Multi-node is deferred; FDB/S3 premature.

### 6.3 Acceptance criteria (keep RocksDB co-location)

All must hold, or open a redesign note in open-choices + DATASTORES:

| Gate | Metric / check | Review trigger |
|------|----------------|----------------|
| **Corpus size** | Total `/var/lib/stalwart-mail` with headroom for compaction | Sustained >70% volume full, or projected full <6 months |
| **Mailbox count / concurrency** | Accounts, parallel IMAP+JMAP+SMTP | Operator-visible latency or journal backpressure under normal use |
| **Restore RTO** | Timed restore drill from restic to empty VM | Exceeds agreed RTO (set when going live; **unset today**) |
| **Backup integrity** | Restore drill proves mail readable via IMAP/JMAP | Any failed drill |
| **FTS quality** | Spot-check search after import and ongoing | Misses/latency unacceptable after tuning language settings |
| **Ops skill** | Operator can stop service, restore `db/`, interpret disk growth | If only SQL tooling is operable, reconsider Postgres for data |
| **HA need** | Single VPS still acceptable | Compliance or uptime requires multi-node |

### 6.4 Open questions (later; RocksDB OK for now)

Operator direction closed "is RocksDB OK?" for this phase (**yes**). Internal
FTS good enough for now; own Surmount search product later. Clever RPO/RTO
later. Remaining (global ids for this file):

**Q1.** Postgres for data later, or stay RocksDB until Surmount search product?
**Q2.** Pin `bufferSize` / `poolWorkers` in Nix before first durable write?
**Q3.** Cold backup only vs volume snapshots when basic restic lands?
**Q4.** Vaultwarden on SQLite (recommended) vs shared Postgres later?
**Q5.** Surmount product DB (sessions, npub map, audit): SQLite under
`/var/lib/surmount` vs none until needed?
**Q6.** When do we set non-default DataRetention / blob cleanup crons?

### 6.5 What would make us leave all-RocksDB

- Failed restore drills or repeated LSM disk/space incidents.
- Need for SQL forensics / PITR as operational requirement.
- Multi-host no longer deferred.
- Internal FTS fails product search gates after honest measurement.

---

## 7. Stack-wide data plane inventory

Every notable state store. "Planned" means documented intent, not necessarily
wired in modules yet.

| Component | Engine | Path / URI | Durability | Backup | Secrets? | Owner |
|-----------|--------|------------|------------|--------|----------|-------|
| Stalwart data+blob+FTS+lookup | RocksDB (provisional) | `/var/lib/stalwart-mail/db` | WAL + process; stop-service backup preferred | restic `mailDataDir`; test restore | Mail passwords inside engine DB | Stalwart |
| Stalwart internal directory | Same RocksDB | inside `db` | Same | Same | Credential hashes | Stalwart |
| Stalwart webadmin cache | Files | `/var/cache/stalwart-mail` | Ephemeral cache | Optional / skip | No | Stalwart package |
| Spam-filter rules | TOML FOD in Nix store | store path via settings | Immutable in closure | Via nix / flake | No | `mail.nix` pin |
| MailPlus import staging | Maildir files | `/var/lib/surmount/import/...` | Files | Optional until import done | May contain mail | Operator |
| Surmount state root | dir | `/var/lib/surmount` | Files | restic `stateDir` | Future sessions etc. | Surmount |
| Management UI durable state | Optional ban/whitelist-last-used JSON; future sessions | default `stateDir/ui/ban-state.json` when accessControl.enable; no secrets | Files | With `stateDir` | Session secrets via sops (future); ban state is IPs only | Surmount management-ui |
| nft surmount_guard sets | nftables sets (empty at install) | kernel nft when accessControl.nftSets | Host firewall | N/A (rebuilt from app/helper) | No | hardening.nix |
| Product auth (npub allowlist) | Config/sops (TBD implementation) | sops +/or stateDir | As designed | sops + state | Mapping may be sensitive | Surmount |
| nginx (transitional) / future Axum-first edge state | certs + conf | conf in Nix; see EDGE_AND_TLS / tls research | Cert material | `/var/lib/acme` (scaffold) | Private keys | Edge module |
| ACME certificates | Let's Encrypt via `security.acme` | `/var/lib/acme/<name>/` | Files (mode-restricted) | restic optional; re-issueable | **Private keys** | Edge / ACME |
| sops age key (host) | age identity file | `/var/lib/sops-nix/key.txt` (default) | File | **Offline copies**; not only restic on same disk | **Critical** | Operator + secrets.nix |
| Deploy secrets (sops scaffold) | age-encrypted YAML or plain files on host | Host paths only (e.g. `/var/lib/surmount/secrets/`); **never public git** | Host + operator channels | Host backup / offline; not the public repo | **Critical**; never commit | Operator + secrets.nix |
| SSH host keys | OpenSSH | `/etc/ssh/ssh_host_*` | Files | Backup carefully; rotation story | **Private keys**; also sops ssh-to-age | OS |
| machine-id | systemd | `/etc/machine-id` | File | Low value; regenerable with care | No | OS |
| journald | journal files | `/var/log/journal` | Ring buffer / vacuum | Usually not full fidelity backup | May leak metadata if mis-logged | OS; host sets `SystemMaxUse` |
| fail2ban | SQLite (typical) + systemd | under `/var/lib/fail2ban` | Local | Optional | No | hardening.nix |
| restic repository | restic encrypted | remote URL (S3/etc.) | Object store | **Is** the backup | Repo password in sops | backups.nix |
| restic cache (local) | cache dir | default restic cache | Disposable | Skip | No | restic |
| Vaultwarden (planned) | **SQLite recommended** for single VPS; Postgres optional | e.g. `/var/lib/bitwarden_rs` (nixpkgs-typical) | DB file or PG | **Critical** restic path | VW admin token + DB from sops; vault contents encrypted client-side | Planned module |
| LUKS2 headers / unlock | LUKS2 | disk header + passphrase/key | Header damage = data loss | Header backup + offline passphrase | **Highest** | Install / SECURITY.md |
| Nix store / generations | Nix | `/nix/store`, profiles | Content-addressed | Not a data backup | Build secrets must not land here plaintext | NixOS |
| Static legacy sites | Files | vhost root TBD | Files | With site content | Usually public | web.nix / open-choices |
| Nostr | **No server-side private key store** | nsec on clients only | N/A | N/A | Never store nsec server-side | open-choices |
| Provider DNS | External | registrar | External | Export zones | API tokens in sops if used | DNS.md |

### 7.1 Vaultwarden engine recommendation (single VPS)

| Engine | When |
|--------|------|
| **SQLite (recommend default)** | One Vaultwarden instance, one VPS, simple backup (stop or careful copy of data dir with VW stopped / consistent). Matches "minimize daemons." |
| **PostgreSQL** | You already run PG for other reasons, need concurrent multi-instance VW, or want PG tooling. Adds coupling and ops surface. |

Vaultwarden does **not** encrypt Stalwart mail data. Client-side vault crypto
protects vault items; server still needs disk/restic hygiene (SECRETS.md).

### 7.2 Management UI / webmail state

Skeleton: essentially stateless proxy. Future likely needs:

- Session store (signed cookie and/or server-side sessions)
- npub <-> account map
- Audit log

Prefer a **small dedicated SQLite** (or files) under `/var/lib/surmount`,
backed up with `stateDir`, rather than stuffing product state into Stalwart
RocksDB. Never a second mailbox corpus.

---

## 8. Backup and migration integrity

### 8.1 Consistent backup order (mail)

Recommended cold-ish procedure:

1. Confirm restic repo and password (sops) work (`restic snapshots`).
2. Prefer maintenance window for first fulls.
3. `systemctl stop stalwart-mail` (accept SMTP/IMAP downtime) **or** atomic
   FS snapshot of the volume holding `mailDataDir` with documented
   consistency assumptions.
4. restic backup at least:
   - `/var/lib/stalwart-mail`
   - `/var/lib/surmount`
   - `/var/lib/acme` (optional; re-issuable)
   - Vaultwarden data dir when present
5. Do **not** rely on restic of a live RocksDB directory without a
   consistency mechanism.
6. Start services; send/receive test message; record snapshot id.

Also back up **offline** (not only in the same restic repo on the same
provider): age identities, LUKS recovery, restic password, VW admin recovery.

### 8.2 Restore drill expectations

Before trusting MX cutover:

1. Fresh VM or wipe data dirs.
2. Install same flake generation (or documented older).
3. Restore restic paths; fix ownership (`stalwart-mail` user).
4. Start Stalwart; authenticate; read known messages via IMAP and/or JMAP.
5. Time the drill; write actual RTO.

A backup never restored is theater ([SECURITY.md](SECURITY.md)).

### 8.3 What not to treat as runtime store

| Artifact | Role |
|----------|------|
| MailPlus Maildir export | **Import source only** |
| MBOX/PST conversions | Import source |
| spam-filter.toml FOD | Config in Nix store |
| git history | Not a mail backup |
| FTS alone | Not a restore path for bodies |

### 8.4 Migration integrity (MailPlus -> Stalwart)

1. Snapshot/export Maildir consistently (MIGRATION.md).
2. Create Stalwart account (directory) first.
3. Import into live store via `surmount-mail-import-maildir` /
   `stalwart-cli`.
4. Verify folder counts, Sent/Drafts, spot-check bodies and attachments.
5. Keep Maildir until verification + backup of **new** store passes.
6. Cut MX only after dual-run confidence.

Duplication risk: re-running import without understanding semantics may
duplicate messages. Operator-run only.

---

## 9. Cross-links and ownership

| Doc / module | Role |
|---------------|------|
| [open-choices.md](open-choices.md) | Proposed / open design |
| [STACK.md](STACK.md) | Short map; points here for depth |
| [OPS.md](OPS.md) | Day-2 ops, disk-full, restic |
| [MIGRATION.md](MIGRATION.md) | Maildir import |
| [SECRETS.md](SECRETS.md) / [SECURITY.md](SECURITY.md) | Layers A/B/C |
| `modules/mail.nix` | Stalwart wiring; store layout choice documented here |
| `modules/backups.nix` | restic paths |
| `modules/options.nix` | `mailDataDir`, `stateDir`, backup knobs |

---

## 10. Explicit unknowns / non-claims

- Exact RocksDB WAL fsync flags inside the Stalwart binary build we ship.
- Production mailbox counts, average message size, attachment mix for
  Surmount.
- Whether nixpkgs will rename `services.stalwart-mail` -> `services.stalwart`
  on 26.05+ (watch release notes).
- Stalwart Enterprise undelete: out of scope unless licensed.
- No claim that internal bloom FTS equals ES quality.
- Live restic without stop/snapshot is unproven for RocksDB consistency; prefer stop or FS snapshot.

When evidence arrives, update **this file** and open-choices in the same turn.
