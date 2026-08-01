# Architecture and datastore review (current tree)

**Date:** 2026-07-30
**Status:** durable peer review of what the tree *is* and what is still open.
Not a decision log. Not operator acceptance of product layout except where
noted below for Fix/FixOS.

## Superseded on host sizing and RocksDB OK-for-now (2026-07-30)

Operator direction the same day updates several assumptions this review
still documents as open or CX-class small-VPS framed:

| Topic | This review (historical peer frame) | Operator direction 2026-07-30 |
|-------|-------------------------------------|------------------------------|
| Host size / provider | Hetzner/OVH-class, CX22-ish framing in places | **Operator-chosen VPS**; ~**16 GB RAM**, **2 TB NVMe**, **16 cores**; not Hetzner-as-default |
| All-RocksDB co-location | Scaffold with heavy exit gates / open pressure | **RocksDB fine for now**; all-role co-location OK this phase |
| Internal FTS | Open vs external FTS pressure | **Good enough for now**; Surmount builds own search product later |
| In-memory on RocksDB | Open | **Fine for now** |
| RPO/RTO backup cleverness | Near-term discussion item | **Later, not now** |
| Edge target | Caddy as next default in places | **No nginx** product edge; prefer **Rust**; Caddy not preferred if Rust is the goal; nginx **transitional-to-delete** |

**Read first for product direction:** [operator-direction.md](operator-direction.md),
[principles.md](principles.md), [packages-and-forks.md](packages-and-forks.md).

This file remains useful for **measured tree facts** (versions, 0.16 config
model, store evidence pointers). Do not treat its host-sizing or
"revisit RocksDB before production" tone as current operator direction.

**Child research (read for depth):**

| Note | Path |
|------|------|
| Stalwart 0.16.15 stores / config evidence | [research/stalwart-0.16.15-stores-evidence.md](research/stalwart-0.16.15-stores-evidence.md) |
| Full scaffold assumption inventory (27 rows) | [research/scaffold-assumptions-inventory.md](research/scaffold-assumptions-inventory.md) |
| Fix / FixOS ladder | [fix-and-fixos.md](fix-and-fixos.md), [research/fix-fixos-ladder.md](research/fix-fixos-ladder.md) |
| Living data plane inventory | [DATASTORES.md](DATASTORES.md) |
| Open / proposed design list | [open-choices.md](open-choices.md) |
| Packaging join | [../.grok/joins/stalwart-current.md](../.grok/joins/stalwart-current.md) |
| Operator direction join | [../.grok/joins/operator-direction.md](../.grok/joins/operator-direction.md) |
| Operator direction doc | [operator-direction.md](operator-direction.md) |

---

## How to read this

Three different kinds of statements appear in this repo. Mixing them up is how
scaffold hardens into accidental product law.

| Kind | Meaning | Examples |
|------|---------|----------|
| **Fact** | Measurable from this tree, upstream docs, or a green build | Stalwart **0.16.15** FOD; `config.json` is DataStore-only; UI default **:8090**; nixpkgs lock rev |
| **Scaffold** | Wired so the host boots or evals green; provisional until you choose | All-RocksDB path; nginx edge (transitional-to-delete); restic module off; sample hostnames |
| **Operator direction** | Dated working direction (2026-07-30 + follow-up); stronger than scaffold when they conflict | RocksDB OK now; Axum-first edge; Nostr; Vaultwarden; deploy secrets; LUKS2; 16G/2TB/16c VPS |
| **Open choice** | Proposed in docs or inventory, not required by working code | Exact edge crate (Axum vs Rama); sops alternative; fork consume path |

**Fix / FixOS (stated agreement only):** You agreed with the Fix/FixOS ownership
ladder direction in [fix-and-fixos.md](fix-and-fixos.md): Surmount may own
packaging and later more of the Nix/NixOS lineage under the names **Fix**
(Nix take) and **FixOS** (NixOS take), climbing L0 through L4 on evidence, not
day-one monorepo theater. That agreement is **about the ladder and naming
direction**, not about every package pin, store layout, UI framework, or edge
choice in this tree.

**Everything else** stays open unless working code cannot run without it (those
are "required by code today," still not permanent product locks). Prefer
**proposed**, **scaffold default**, **research finding**, **open**,
**operator-deferred** until you say otherwise in writing.

When code and living docs disagree, **code + packaging joins win** for version
and ports; doc lag is listed in section 6.

---

## 1. Versions and sources

Measured 2026-07-30 from package files, `nix build`, `--version`, and
`flake.lock`. Detail:
[research/stalwart-0.16.15-stores-evidence.md](research/stalwart-0.16.15-stores-evidence.md),
[../.grok/joins/stalwart-current.md](../.grok/joins/stalwart-current.md).

| Component | Version / value | Packaging / source | Path |
|-----------|-----------------|--------------------|------|
| Stalwart server | **0.16.15** | Release binary FOD (`stalwart-{arch}-unknown-linux-gnu.tar.gz`) | `nix/packages/stalwart-mail.nix` |
| Packaging mode | `release-binary-fod` | `passthru.packagingMode` | same |
| stalwart-cli | **1.0.12** | Release binary FOD | `nix/packages/stalwart-cli.nix` |
| WebUI assets | **1.0.7** | Zip FOD (`webui.zip`) | `nix/packages/stalwart-webui.nix` |
| spam-filter rules | **3.0.0** | toml + `spam-filter-rules.json.gz` FODs | `nix/packages/stalwart-spam-filter.nix` |
| Host OS channel | **nixos-25.05** | flake input; engine **not** from channel | `flake.nix`, `flake.lock` |
| nixpkgs lock rev | `ac62194c3917d5f474c1a844b6fd6da2db95077d` | locked node `nixpkgs` | `flake.lock` |
| Channel `pkgs.stalwart-mail` | still **0.11.8** on 25.05 | historical only; disabled module | nixpkgs |
| Surmount service module | custom | disables nixpkgs TOML module | `modules/stalwart-service.nix` |
| Management UI | Axum + Leptos SSR admin shell scaffold (ssr-only; no hydrate) | crane under `nix/packages/` with `rustPackages_1_88` | `crates/management-ui/`, `nix/packages/management-ui.nix`, `nix/rust-toolchain.nix` |
| Upstream server tag | `v0.16.15` (published 2026-07-27) | https://github.com/stalwartlabs/stalwart | evidence note |
| RocksDB (upstream lock) | rust `rocksdb` **0.24.0**; `librocksdb-sys` **0.17.3+10.4.2** | tag `Cargo.lock` | evidence note |
| Blob hash crate | `blake3` **1.8.5** | tag `Cargo.lock` | evidence note |

**Why not stay on 0.11.8:** nixos-25.05 packaged 0.11.8. That was a scaffold
accident, not a product pin. Greenfield does not need old-engine compatibility
([AGENTS.md](../AGENTS.md), [open-choices.md](open-choices.md)).

**Why binary FOD, not rustPlatform today:** cargo vendor hit crates.io HTTP 403
in this environment; CLI wants rustc newer than 25.05's 1.86 for some unstable
lib features. Binary FODs are hermetic hashes and match the published tag.
Source build is a future path, not current.

**Approx closures (this machine after build):** server ~126.5 MiB; cli ~39 MiB
(see evidence note for store path examples).

---

## 2. What the mail engine actually is on 0.16.15

### 2.1 Config model (the hard break from 0.11)

From upstream `UPGRADING/v0_16.md` at tag v0.16.15 and Surmount modules:

1. **No TOML.** On-disk config is a small **`config.json`** that is **only a
   DataStore** JSON object (`@type` + path and optional RocksDB knobs).
2. **Everything else** lives in the datastore as **JMAP objects**: listeners,
   accounts, spam, TLS, BlobStore / SearchStore / InMemoryStore choices,
   WebUI resource URL, etc.
3. Day-2 admin is **WebUI** or **`stalwart-cli apply`** (plus get/update/query).
4. Old REST management API is gone; management is JMAP at `/jmap`.
5. Account names must be **email addresses** in 0.16 (greenfield constraint for
   how we create principals).

Official storage entry points:

- https://stalw.art/docs/storage/
- https://stalw.art/docs/install/store/
- https://stalw.art/docs/ref/object/data-store
- Upgrade: https://github.com/stalwartlabs/stalwart/blob/v0.16.15/UPGRADING/v0_16.md

### 2.2 Four logical store roles (upstream fact)

| Role | 0.16 object | Holds |
|------|-------------|-------|
| **Data** | `DataStore` | Metadata, folders, headers/refs, domains, most config objects |
| **Blob** | `BlobStore` | Large binaries: messages, attachments, Sieve (BLAKE3 content-addressed) |
| **Search** | `SearchStore` | Full-text indexes (internal Default, or ES / Meilisearch / SQL variants) |
| **In-memory** | `InMemoryStore` | Soft KV: rate limits, greylist, locks, short-lived tokens |

Runtime still has typed enums in source (`Store`, `BlobStore`, `SearchStore`,
`InMemoryStore` at v0.16.15). Directory is **not** a fifth store role: internal
directory rows live in the data store; external LDAP/SQL/OIDC are optional.

**Backend change after go-live requires data migration** (upstream warning).
Choose physical layout before first durable production write when you can.

### 2.3 Backend matrix (upstream docs)

| Backend | Data | Blob | Search | In-memory |
|---------|------|------|--------|-----------|
| RocksDB | yes | yes | yes | yes |
| FoundationDB | yes | yes | yes | yes |
| PostgreSQL | yes | yes | yes | yes |
| MySQL / MariaDB | yes | yes | yes | yes |
| SQLite | yes | yes | yes | yes |
| S3 / MinIO / Azure / filesystem | | yes | | |
| ElasticSearch / Meilisearch | | | yes | |
| Redis / Valkey | | | | yes |

Single-node docs treat **RocksDB for all four** as a normal lightweight layout.
Distributed sketch: FoundationDB + S3 + Redis + ES/Meilisearch.

### 2.4 What Surmount wires today

| Item | Value |
|------|--------|
| Generated `config.json` | `{"@type":"RocksDb","path":"/var/lib/stalwart-mail/db"}` |
| `blobSize` / `bufferSize` / `poolWorkers` | omitted; upstream defaults apply |
| Upstream RocksDB defaults (docs) | `blobSize` **16834**, `bufferSize` **134217728** (128 MiB), `poolWorkers` = CPU count |
| dataDir | `/var/lib/stalwart-mail` |
| Unit | `stalwart-mail.service` -> `stalwart --config=<config.json>` |
| User | `stalwart-mail` |
| nixpkgs module | **disabled**; Surmount owns `modules/stalwart-service.nix` |
| Option path name | still `services.stalwart-mail` (same path, new implementation) |
| `settings` attr | accepted and **ignored** (no TOML writer) |
| Blob / Search / InMemory in Nix | **not declared**; after first boot follow upstream **Default** (same data store) unless apply/WebUI changes them |
| Hermetic FODs on host | `/etc/surmount/stalwart/{spam-filter.toml,spam-filter-rules.json.gz,webui.zip}` |
| FOD runtime wiring | **not auto-applied**; first boot may still hit GitHub until apply/WebUI |
| Product access to engine DB files | **out of bounds**; JMAP / admin / CLI only |

### 2.5 First-boot listeners (upstream, not Nix-declared)

Empty RocksDB + safe defaults (join + module comments):

| Port | Role | Notes |
|------|------|--------|
| 25 | SMTP | public intent |
| 465 | SMTPS submission | public intent |
| 993 | IMAPS | public intent |
| 4190 | ManageSieve | public intent |
| **8080** | HTTP management / JMAP | often `[::]:8080`; **production gap** until rebound to loopback |
| **443** | HTTPS (engine) | **collides** with edge-owned :443 |
| 995 | POP3S | may be unwanted |
| 587 / 143 | submission / IMAP plain | **not** default in 0.16 safe defaults |

Surmount management UI default moved **8080 -> 8090** so it does not fight
Stalwart HTTP. nginx proxies UI on 8090 and `/stalwart-admin/` to
`127.0.0.1:8080`. Env: `SURMOUNT_STALWART_URL=http://127.0.0.1:8080`.

Plain 587 is a common MUA expectation; if you need it, it is an apply-time
listener choice, not something Nix pins today.

---

## 3. Datastore deep dive (question every assumption)

Depth inventory and ops gates also live in [DATASTORES.md](DATASTORES.md).
This section is the challenge pass: upstream fact, Surmount wire, unproven
agent assumptions, operator questions.

### 3.1 Design posture (agreed process, not backend choice)

1. Every store is a choice. Channel defaults are proposals.
2. Logical four-store model first; physical backend second.
3. No Surmount load test under production mailbox counts yet. Numbers are
   design intent and upstream guidance, not SLOs.
4. Product code does not open engine DB files.
5. MailPlus Maildir is **import source only**; runtime authority is Stalwart.

### 3.2 Data store (metadata + config objects + internal directory)

| | |
|--|--|
| **Upstream** | Structured source of truth for what exists; must stay consistent with blob refs. Internal directory defaults into the data store. https://stalw.art/docs/storage/data , https://stalw.art/docs/auth/backend/internal/ |
| **We wire** | One RocksDB DataStore at `/var/lib/stalwart-mail/db`. Internal directory implied by Default Authentication (not separately declared in Nix). |
| **Not proven** | That RocksDB remains the right metadata engine after real corpus size, SQL forensics need, or multi-app shared PG. That directory must stay co-located forever (upstream warns against incompatible mixes). Exact first-boot Authentication / directory singleton JSON not captured from a live VM in the evidence pass. |
| **Questions** | Keep internal directory for day-one mail auth? Any requirement for LDAP/OIDC/SQL directory before MX? Prefer SQL tooling (Postgres) for metadata forensics/PITR before first durable write? |

### 3.3 Blob store (message bytes)

| | |
|--|--|
| **Upstream** | Content-addressed **BLAKE3** (32-byte). Small objects may inline in data store below `blobSize` (default **16834**). App-level Email `compressionAlgorithm` default **LZ4**. Variants: Default (data store), S3, Azure, filesystem, SQL, FDB, sharded enterprise. https://stalw.art/docs/storage/blob |
| **We wire** | Implicit Default -> same RocksDB. No separate blob path, S3, or filesystem tree. |
| **Not proven** | Optimal `blobSize` for Surmount attachment mix. Whether VPS disk economics force object storage before multi-host. RocksDB blob-file CF behavior under our FOD (source uses blob files + min blob size; we did not audit fsync flags in the stripped binary). |
| **Questions** | Stay on-disk co-located blobs through first production year? Any compliance need for blob tiering or separate volume? Accept upstream `blobSize` 16834 until measured? |

### 3.4 Search / FTS store

| | |
|--|--|
| **Upstream** | Default = internal index in data store (docs: bloom-filter style; higher write amp than dedicated engines). External: ElasticSearch, Meilisearch (note Meilisearch `maxTotalHits` default 1000 can truncate "all matches"). PG FTS truncates ~650 KB body + attachments. https://stalw.art/docs/storage/fts/ |
| **We wire** | Implicit Default. Product search docs propose JMAP `Email/query` only; no parallel Surmount mail index. JMAP proxy route is **501**. |
| **Not proven** | Internal FTS quality/latency on real MailPlus corpus. Whether roaring/bitpacking + bloom wording matches operator mental model enough for support. Rebuild/reindex exact CLI path on cli **1.0.12** (version-dependent; verify before needing it). |
| **Questions** | Any mail search quality bar that forces external FTS before v1 webmail? Accept "no parallel mail FTS" as product rule? |

### 3.5 In-memory / lookup store

| | |
|--|--|
| **Upstream** | Soft state: rate limits, greylist, locks, OAuth/ACME tokens, etc. Default = data store; Redis for heavy/distributed. Loss on restart often acceptable; durable backend needs cleanup schedules. https://stalw.art/docs/storage/in-memory |
| **We wire** | Implicit Default on same RocksDB. |
| **Not proven** | Whether greylist/rate-limit write load on co-located RocksDB is hot under real mail volume (16 GB host should be comfortable vs tiny VPS). No Redis module in tree. |
| **Questions** | Fine with ephemeral limits resetting on restart? Any reason to add Redis on a single VPS? |

### 3.6 Physical backend options (when each might earn a seat)

#### RocksDB (current scaffold)

- **Process:** in-process LSM inside `stalwart-mail`. Single writer owns the directory.
- **Backup:** stop service or consistent FS snapshot, then restic of `mailDataDir`. Live copy of open RocksDB is **risky** (no clear operator-facing online checkpoint in docs we reviewed).
- **Ops cost:** lowest daemon count; pay in LSM literacy (write amp, space amp, disk still full after delete).
- **Failure modes:** disk full mid-compaction; two writers; hot backup restores garbage; OOM if buffers oversized for RAM.
- **Assumption to challenge:** "single-node recommended" is true as upstream guidance; it is **not** proof that Surmount's RPO/RTO, disk, or ops skill match that layout.

#### SQLite

- Embedded; simpler mental model; weaker writer concurrency under heavy parallel SMTP+IMAP.
- nixpkgs **legacy** path was SQLite + filesystem blobs for old `stateVersion`; we are not on that path.
- Revisit only if RocksDB ops pain dominates and scale stays tiny.

#### PostgreSQL

- Mature PITR, SQL forensics, multi-connection. Extra unit on one VPS.
- Strong candidate **if** you already want PG for Vaultwarden or ad-hoc SQL, or RocksDB restore/ops fail gates.
- FTS-via-PG has size truncation; usually keep search Default or external if leaving RocksDB only for data.

#### MySQL / MariaDB

- No Surmount preference over Postgres if an RDBMS is chosen. Harsher FTS limits.

#### FoundationDB

- Distributed ACID path. Wrong cost class while multi-host is deferred.

#### S3 / MinIO / Azure / filesystem blobs

- Earn a seat when blob growth outstrips VPS disk or multi-node appears. Adds credentials, latency, partial-failure modes (`verifyAfterWrite` default true on S3 per docs).

#### Elasticsearch / Meilisearch

- Only after internal FTS fails measured gates. ES: JVM tax on small VPS. Meilisearch: raise `maxTotalHits` for mail semantics.

#### Redis / Valkey

- In-memory role only. Extra moving part on one VPS unless shared limits or write heat demand it.

### 3.7 Co-location vs split (decision aids)

**Stay co-located RocksDB while** corpus fits with compaction headroom, single
operator wants one backup tree, single VPS still acceptable, FTS quality OK.

**Consider split when** blobs dominate disk economics, you want PG PITR for
metadata only, internal FTS fails gates, or multi-host is no longer deferred.

**Acceptance gates** (from DATASTORES; RPO/RTO still **unset**): disk headroom
(~70% sustained / projected full), restore drill time, backup integrity via
IMAP/JMAP readback, FTS spot-checks, ops skill with stop-and-restore, HA need.

### 3.8 Stack-wide stores outside the engine

| Component | Engine / path | Status |
|-----------|---------------|--------|
| Import staging | Maildir under `/var/lib/surmount/import/...` | Disposable after verified import |
| Surmount product state | `/var/lib/surmount` (future sessions, npub map, audit) | Path exists; product DB **not** implemented |
| ACME material | `/var/lib/acme/` via `security.acme` | Wired for nginx; Stalwart mail TLS file use **TODO** |
| sops age key | host identity (default `/var/lib/sops-nix/key.txt`) | Module defaults; real `secrets.yaml` still scaffold |
| restic repo | remote URL when enabled | Module present, **off** until repo + password set |
| Vaultwarden | planned; SQLite recommended on single VPS | Docs only; no module |
| Spam-filter FOD | Nix store + `/etc/surmount/stalwart/` | Config, **not** message DB |
| Nostr | nsec on clients only | Never server-side nsec |

Full table: [DATASTORES.md](DATASTORES.md) section 7.

### 3.9 Unverified on the 0.16.15 binary (do not paper over)

From evidence section 4:

1. Exact Cargo features of the published GNU tarball vs upstream Dockerfile line
   (binary strings suggest multi-backend; no rebuild manifest).
2. Whether FoundationDB is *usable* in the FOD without extra cluster libs.
3. Live first-boot singleton JSON for Blob/Search/InMemory (inferred Default).
4. Internal FTS behavior under real load (no red/green search test on this pin).
5. RocksDB CF compression codec in 0.16 JSON world (0.11 TOML had explicit lz4;
   0.16 documented RocksDB JSON fields are path / blobSize / bufferSize /
   poolWorkers; app blob LZ4 is separate).
6. Maildir import subcommand on cli **1.0.12** (helper still templates
   `import messages --format maildir-nested`; schema-driven CLI may differ).
7. Full `mail-vm-test` not run in the evidence packaging pass.

---

## 4. Full stack assumption review

Format: **claim** | **evidence** | **challenge** | **question**.
Full row inventory:
[research/scaffold-assumptions-inventory.md](research/scaffold-assumptions-inventory.md).

### 4.1 Mail engine and packaging

| Claim | Evidence | Challenge | Question |
|-------|----------|-----------|----------|
| Mail engine is Stalwart only | `modules/mail.nix`, unit, firewall, import helper, UI probes Stalwart HTTP | Required by code for *this* path; engine *choice* vs Postfix+Dovecot etc. is still proposed | Keep Stalwart as sole production engine for cutover? |
| Pin **0.16.15** via release-binary FOD | packages + green builds; join | Packaging choice, not "binaries forever"; trust model and bump ownership open | Is binary FOD OK until hermetic source works, or must source build precede live MX? |
| Surmount owns service module; nixpkgs TOML module off | `stalwart-service.nix` `disabledModules` | Required for 0.16 to run on this flake; declarative apply plans still missing | How much first-boot/day-2 config is in-repo apply plans vs WebUI clicks? |
| spam/webui FODs installed, not auto-applied | `mail.nix` environment.etc; join known gap | Impure GitHub fetch on first boot possible | Require hermetic spam/webui on first boot? |

### 4.2 Data plane

| Claim | Evidence | Challenge | Question |
|-------|----------|-----------|----------|
| All four roles on one RocksDB | config.json path only; Defaults for other roles | Scaffold; **operator direction 2026-07-30: RocksDB fine for now**, co-location OK this phase | Still open later: when (if ever) to split; not a Day-1 blocker |
| No PG/S3/ES/Redis/FDB in modules | tree grep / modules | Absence is not a forever ban; blob tiering **not now** per operator | Defer backends until measurement or new operator direction |
| Search owned by Stalwart FTS | SEARCH_AND_UI, DATASTORES | Internal FTS **good enough for now**; Surmount own search product later | No external FTS for convenience; quality bar only when building Surmount search |
| Product state under `/var/lib/surmount`, not in engine DB | options `stateDir`; DATASTORES | Boundary is sound; session store shape open | Cookie vs server session store when auth lands? |

### 4.3 Identity and UI

| Claim | Evidence | Challenge | Question |
|-------|----------|-----------|----------|
| Nostr-first product auth (npub + NIP-98) | open-choices, SECURITY, STACK; **operator direction** | **Code still missing**; UI has no auth gate, no Nostr deps | Bootstrap allowlist and key-loss story still open (Q-level) |
| Stalwart keeps mail passwords / app passwords | directory model; human tracking via Vaultwarden (direction) | Bridge issuance under 0.16 APIs not built | Who issues app passwords day one (WebUI vs future Surmount)? |
| Axum shell + Leptos SSR invested path | docs + **operator direction**; admin `GET /` is Leptos SSR scaffold | Hydrate islands + richer admin still residual | Phase 2 admin features + Nostr |
| Admin first, then real webmail in v1 | SEARCH_AND_UI; **operator direction yes** | Phase order directed; not fully built | Cutover scope detail only |
| UI :8090, Stalwart HTTP :8080 | options, management-ui, web.nix | Required by current defaults; public :8080 is a gap | Keep this split, or permanent private port via apply? |

### 4.4 Edge, TLS, Cloudflare

| Claim | Evidence | Challenge | Question |
|-------|----------|-----------|----------|
| nginx today; **Axum-first edge target**; nginx transitional-to-delete | `web.nix` default off; EDGE_AND_TLS; management-ui rustls; **operator direction** | Axum rustls edge foundation in tree (`listenMode=https`, rate-limit, ban); nginx module residual dual-run escape (`web.enable=true`); host public cutover + MDWE still operator residual | Prefer Axum-only public cutover; keep dual-run only as escape until operators drop it |
| No required Cloudflare hop | open-choices, EDGE, SECURITY, hygiene | Design posture in modules (origin ports); not a runtime dep | Confirm no-required-CF; any CF product intentionally later? |
| ACME for web; Stalwart mail TLS TODO | web.nix ACME; host TODOs | Cert objects are apply/datastore territory; :443 fight with engine defaults | Same ACME material for IMAPS/SMTPS? Who owns renewal on Stalwart side? |
| Static legacy sites only | web.nix extraVhosts; open-choices; **operator direction: no exceptions** | No staged content roots yet | Which hostnames/roots at cutover? |

### 4.5 Secrets, backups, host topology

| Claim | Evidence | Challenge | Question |
|-------|----------|-----------|----------|
| Secrets buckets: deploy secrets / Vaultwarden / LUKS disk | SECRETS, secrets.nix; **operator direction** | Deploy secrets: sops-nix scaffold (need real; tool can change); VW + LUKS directed | Keep sops-nix or replace tool (Q-DEP-1)? When do VW and LUKS become real work? |
| restic opt-in | backups.nix disabled until repo+password | Scaffold; **clever RPO/RTO later, not now** (operator) | Where does the restic repo live? Basic backup before MX still wise |
| Single VPS; ends when operator says | open-choices, hosts/mail-vps; **operator direction** | No invented scale-out triggers | Do not invent multi-host exit criteria |
| Operator-chosen VPS ~16G/2TB NVMe/16c; PTR; LUKS first-class | operator-direction.md | Preference/direction text, not automation | Actual provider (Q-HOST-1)? First install LUKS or interim plain disk (Q-HOST-2)? |
| Firewall 22/25/80/443/465/587/993/4190 | networking.nix | Scaffold perimeter; 0.16 may still bind 8080/443/995 broadly | Lock SSH to admin nets? Open POP3 995 at all? |

### 4.6 Fix level, migration, ops, flake

| Claim | Evidence | Challenge | Question |
|-------|----------|-----------|----------|
| Fix/FixOS ladder; today L0 + early L1 | fix-and-fixos.md; Stalwart FODs + modules are L1 evidence | **You agreed ladder direction**; L2 soft channel and L3/L4 not started | Stay L0+L1 inside this repo for now, or split packaging/soft channel while greenfield is cheap? |
| Maildir-nested import, operator-run only | mail.nix helper; MIGRATION | Helper is template; cli 1.0.12 import path needs confirm | Only import path needed? Who runs verified trial import? |
| Hermetic flake; host on 25.05; crane UI | flake.lock; packages | Channel pin is scaffold; source Stalwart may force newer rustc later | Stay on 25.05 until a concrete package need, or bump host channel sooner? |
| Self-ops: journald, scripts, health | OPS, scripts/, UI /health | Partial scaffold; no SaaS log sink required | External monitoring required before go-live? |
| Domains: surmount.systems / mail / services | options defaults; sample host | Scaffold names; DNS not proven by inventory | Hostnames and bootstrap local-parts final? |

---

## 5. Recommended discussion order

Walk these in order so early choices constrain later ones without locking what
you have not discussed. **This section does not choose for you.**

1. **Engine stay**
   Stalwart as sole mail engine for production cutover, or is a classic stack
   still on the table?

2. **Packaging trust model**
   Release-binary FOD until source vendor/rustc work, vs must-build-from-source
   before MX. Who owns version bumps?

3. **Data plane (mostly directed; finish knobs)**
   RocksDB co-location OK for now. Set basic backup (stop vs volume snapshot),
   restic location. Clever RPO/RTO later. blobSize/bufferSize/poolWorkers
   starting points in operator-direction.md. Internal directory for mail auth.

4. **Listener and port reality (0.16)**
   Public mail ports; Stalwart HTTP local only (UDS preferred); whether engine
   :443 exists at all; POP3; submission :587; UI port split. Plan apply plans
   for listeners before public MX/HTTPS.

5. **Edge and TLS**
   nginx bridge vs Axum-first edge before MX; UDS backends; cert material shared
   with Stalwart file TLS; no-required-CF confirmation. ACME not locked sole
   path (tls-trust-and-acme research). Separate proxy products not preferred.

6. **Identity and UI phases**
   Nostr-first (directed); admin-then-webmail (directed); Leptos (directed);
   bootstrap allowlist / key loss still open (Q-AUTH-1).

7. **Secrets and disk**
   Encrypted deploy secrets (sops-nix or alternative tool) + Vaultwarden +
   LUKS; offline recovery copies (age, restic password, LUKS).

8. **Migration and cutover ops**
   Confirm Maildir path on cli 1.0.12; trial import owner; dual-run bar;
   hermetic spam/webui apply requirement. Priority: recover MailPlus data.

9. **Host topology and Fix level**
   Provider pick on ~16G/2TB/16c shape; single VPS until operator says; stay
   Surmount package overlay in-repo vs early fixpkgs channel.

10. **Self-ops bar**
    journald+scripts enough, or external uptime/metrics before go-live.

---

## 6. What we should stop implying until decided

Doc and comment lag can cause wrong operator actions. Prefer one story:
**0.16.15**, **config.json DataStore**, **UI :8090**, **Stalwart HTTP :8080**,
**spam-filter 3.0.0**, **Surmount-owned module**.

### 6.1 Doc / join cleanup candidates

| Location | Stale implication | Current fact |
|----------|-------------------|--------------|
| `docs/STACK.md` diagram and ports table | UI :8080, Stalwart HTTP :8081 | UI **:8090**, Stalwart **:8080** |
| `docs/MIGRATION.md` | :8081 references | Management HTTP **:8080** |
| `docs/STACK.md` / README / open-choices engine wording | "current (see package after bump)" without number | Pin is **0.16.15** (still not "accepted forever") |
| `docs/DATASTORES.md` section 5.x | TOML-shaped eval (`storage.data = "db"`, spam v2.0.5, nixpkgs module settings) | 0.16 config.json; spam-filter **3.0.0**; Surmount module |
| `docs/fix-and-fixos.md` section 3 "today" | Stalwart still largely from nixpkgs until packaging lands | Packaging **landed** (L1 evidence): Surmount FODs + service module |
| `.grok/joins/foundation.md` | Older port/module story | Defer to `stalwart-current.md` + modules |
| README "Stalwart admin fallback" ssh example | `-L 8081:127.0.0.1:8081` | Should be **8080** (and note public-bind gap) |
| open-choices "Engine version" | "write current after bump" / TODO package path | Package paths exist; version **0.16.15** |

### 6.2 Code / ops implications to stop treating as settled product law

| Implication | Reality |
|-------------|---------|
| "We decided all-RocksDB" | Scaffold default with gates; open until you accept |
| "settings TOML still configures Stalwart" | `settings` ignored; apply/WebUI only |
| "spam-filter FOD is live anti-spam on first boot" | Files on disk; engine may still use GitHub until apply |
| "import helper is production-ready" | Template; confirm cli 1.0.12 subcommands |
| "edge owns 443 exclusively" | True for nginx intent; engine safe defaults may also bind 443 until rebind |
| "Caddy / Nostr auth / Leptos hydrate+webmail / Vaultwarden are implemented" | Caddy/Nostr/Vaultwarden not product code; Leptos admin shell is ssr-only scaffold in code; hydrate+webmail residual |
| "multi-host never" | Deferred, not forbidden |
| "RPO/RTO exist" | Unset |
| "mail TLS is done because ACME exists for nginx" | Web certs != Stalwart protocol cert wiring |

### 6.3 Suggested cleanup posture

When you accept or reject a row, update [open-choices.md](open-choices.md) and
the owning living doc **in the same turn**. Do not treat research files or this
review as acceptance. A dedicated docs-sync pass (STACK, MIGRATION, DATASTORES
section 5, README ports, fix-and-fixos "today") is pure lag burn-down and does
not require choosing backends first.

---

## See also

| Doc | Role |
|-----|------|
| [open-choices.md](open-choices.md) | Proposed / open list (points here) |
| [DATASTORES.md](DATASTORES.md) | Living datastore inventory + gates |
| [STACK.md](STACK.md) | Short layer map (may lag ports; prefer this review + modules) |
| [fix-and-fixos.md](fix-and-fixos.md) | Ladder (stated agreement on direction) |
| [EDGE_AND_TLS.md](EDGE_AND_TLS.md) | Edge / ACME / no CF |
| [SECRETS.md](SECRETS.md) / [SECURITY.md](SECURITY.md) | Secrets layers and threat notes |
| [MIGRATION.md](MIGRATION.md) | MailPlus Maildir import |
| [OPS.md](OPS.md) | Day-2 ops posture |
| [SEARCH_AND_UI.md](SEARCH_AND_UI.md) | Search ownership and UI phases |
| [research/](research/) | Version-pinned evidence |
