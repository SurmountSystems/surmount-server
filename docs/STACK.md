# Surmount Server stack map

Living architecture map for the mail + web VPS. Design notes that should
survive compaction live here, in [operator-direction.md](operator-direction.md),
and in [open-choices.md](open-choices.md). Implementation detail stays in Nix
modules and Rust crates.

**Last updated:** 2026-07-30

Operator direction 2026-07-30 is working product direction (dated, not eternal
law). Scaffold defaults still describe what code does today when they differ.

## Goals in one paragraph

Run mail, light static web, admin console, and v1 webmail on a **single
operator-chosen VPS** (~16 GB RAM, 2 TB NVMe, 16 cores as directed). **Stalwart**
is the mail engine (version from `nix/packages/stalwart-mail.nix`; currently
**0.16.15** binary FOD). **RocksDB** for all store roles is **fine for now**.
Our Rust product layer (**Axum + Leptos SSR**) is operator admin first, then
real webmail, with **Nostr** authentication at the product edge. HTTPS must
move to an **Axum-first** edge we own (nginx is transitional-to-delete; no
required Cloudflare hop). Prefer **Unix domain sockets** between local
services. **Arti onion/hidden services are REQUIRED** (first-class reachability
alongside clearnet). **Encrypted deploy secrets** must be available at
activation (**sops-nix** today); humans use planned **Vaultwarden**; disk at
rest prefers **LUKS2**. Multi-host ends when the operator says. Compaction
reload: [COMPACTION-PIN.md](COMPACTION-PIN.md).

## Layer diagram

```text
                    Internet (direct to VPS; no required CDN hop)
                              |
         +--------------------+--------------------+
         |                    |                    |
    :25/:465/:587         :80/:443              :993/:4190
    SMTP / submission     HTTPS edge            IMAPS / ManageSieve
         |                    |                    |
         |              +-----v------+             |
         |              | Edge / TLS |  (nginx     |
         |              | HTTPS      |   transitional-to-delete;
         |              +-----+------+   target Axum-first + UDS;
         |                    |          EDGE_AND_TLS.md)
         |         +----------+----------+----------+
         |         |                     |          |
         |    UDS preferred         UDS preferred   | (future)
         |    management-ui         Stalwart HTTP   Vaultwarden
         |    Axum + Leptos SSR     (JMAP + /admin) local only
         |    Nostr product auth         |          |
         |         |                     |          |
         +---------+----------+----------+----------+
                              |
                    Stalwart mail engine
                    (SMTP/IMAP/JMAP/Sieve;
                     mail accounts + passwords/tokens;
                     internal FTS good enough for now)
                              |
              +---------------+---------------+
              |               |               |
         RocksDB data    RocksDB blob    RocksDB FTS
         (+ lookup/dir)  (message bytes) (search index)
         under /var/lib/stalwart-mail/db  (OK for now; co-located)

  Optional static vhosts (legacy sites): edge serves files only
  Disk: prefer LUKS2 root (FDE) when install allows; see SECURITY.md
  Host: operator-chosen VPS (not Hetzner-as-default)
```

## Ownership

| Concern | Owner | Notes |
|---------|-------|-------|
| SMTP, submission, IMAP, ManageSieve, JMAP | **Stalwart** | Do not fork a parallel MTA or message store |
| Spam filter, greylisting, DKIM signing | **Stalwart** | First-class spam/security priority; rules FOD in packages |
| Message store + internal FTS (for now) | **Stalwart** | Surmount builds own search product later |
| Mail account directory (local parts, app passwords) | **Stalwart** | Engine credentials; human inventory via Vaultwarden |
| Product identity / login (operators, users) | **Surmount (Nostr)** | Keys via host OS / other Surmount tools |
| Bridge: Nostr session -> Stalwart APIs | **Surmount** | Provision/authorize; never browser-hold Stalwart admin |
| TLS on mail ports (465/993 and STARTTLS) | **Stalwart** (certs from ACME paths) | Operator TODO until cert paths wired |
| HTTPS edge, certs for web vhosts | **management-ui** rustls (default); nginx dual-run escape | `web.enable` default false; nginx transitional-to-delete |
| Management console + v1 webmail HTTP | **surmount-management-ui** | Axum shell + Leptos SSR |
| User-facing mail UX (search, read, compose) | **Product via JMAP** | After admin; Stalwart internal search for now |
| Legacy / migrated public sites | **Static files only** | No exceptions for old Synology apps |
| Non-mail product indexes (audit, ops logs) | **Surmount** (only if needed) | Explicit product need required |
| Deploy / Nix secrets | **sops-nix** (or alternative tool) | Bucket 1; decrypt on host at activation |
| Human / team password vault | **Vaultwarden** (planned) | Bucket 2; PM API; not Bitwarden Secrets Manager; not deploy-secrets |
| **Arti onion / hidden services** | **Arti (REQUIRED)** | Tor Project Rust Tor; HS reachability first-class next to clearnet; not a clearnet edge replacement; HS keys never in git |
| Disk encryption at rest | **LUKS2** (first-class) | Disk encryption; SECURITY.md |
| Packaging / OS / deploy | **Nix flake + Surmount package overlay** | Hermetic lock; path to fixpkgs later; **no Python/NPM product deps** |
| Stalwart patches | **SurmountSystems/stalwart fork** when needed | Operator-managed consume; packages-and-forks.md |
| Migration MailPlus -> Stalwart | **Operator-run** | `docs/MIGRATION.md`; never silent activation |
| Backups | **restic** (opt-in module) | Basic now; clever RPO/RTO later |
| Host hardening | **hardening.nix** | SSH, transitional fail2ban; merciless ban+whitelist target (research/access-control-fail2ban.md) |
| Mail DNS legitimacy | **Operator + DNS.md checklist** | SPF/DKIM/DMARC/PTR; MTA-STS/TLS-RPT; DNSSEC+DANE plan |

## Identity and auth (summary)

```text
  Human (extension / signer; keys via host OS / Surmount tools)
           |
           | NIP-98 (kind 27235) or login challenge -> session
           v
  Axum / Leptos  ---- maps npub -> role + mail account(s)
           |
           | management token / app password / JMAP as designed
           v
  Stalwart directory + store
           ^
           |
  Classic MUA (Thunderbird, etc.): IMAP/SMTP app password
  (may be issued/rotated by Surmount after Nostr admin login;
   human tracking in Vaultwarden)
```

- **Do not** claim Stalwart natively verifies Nostr signatures.
- Stalwart **can** use external OIDC/LDAP/SQL directories; optional later
  bridge, not v1 requirement. See open-choices and SECURITY.md.
- Multi-host IdP topology: deferred until operator ends single-VPS phase.

## Data stores (summary)

**Depth for DB engineers:** [DATASTORES.md](DATASTORES.md).
**Direction:** [operator-direction.md](operator-direction.md) (RocksDB OK now).

### Stalwart four-store model (short)

Official: [Choosing a database](https://stalw.art/docs/install/store/),
[Storage](https://stalw.art/docs/storage/),
[RocksDB](https://stalw.art/docs/storage/backends/rocksdb/).

| Store | Role | Surmount physical (now) |
|-------|------|-------------------------|
| **Data** | Metadata, folders, headers, config | One RocksDB at `/var/lib/stalwart-mail/db` |
| **Blob** | Message bodies, attachments (BLAKE3 CAS) | Same RocksDB (Default) |
| **Search (FTS)** | Internal FTS (good enough for now) | Same RocksDB |
| **In-memory / lookup** | Rate limits, greylist, locks, short TTL KV | Same RocksDB |
| **Directory** | Internal principals/credentials | Same data store |

Live engine version: `nix/packages/stalwart-mail.nix`. Spam-filter FOD is
**not** the message DB. Maildir import staging is **not** the runtime store.

**RocksDB knobs** (blobSize / bufferSize / poolWorkers) starting points for
the 16 GB / 16-core box: [operator-direction.md](operator-direction.md)
section 2. Module options today: `blobSize` / `bufferSize` optional overrides.

No blob tiering/compliance split this phase. Clever RPO/RTO backup design later.

### Product / UI / other state

- Management multi-page console: little durable state today; future
  sessions/npub map/audit under `/var/lib/surmount` (inventory when added).
  HTML routes `/`, `/domains`, `/accounts`, `/system`, `/mail`; honest JSON
  inventories; JMAP proxy remains 501 residual.
- Never a second mailbox corpus in Surmount code.
- Vaultwarden (planned): own data dir; **SQLite recommended** on single VPS.
- Full table (ACME, sops age keys, journald, fail2ban, restic, LUKS, SSH
  host keys, etc.): DATASTORES.md section 7.

## Secrets buckets (do not mash)

| Bucket | What | Tool |
|--------|------|------|
| **1** | Service secrets at NixOS activation | **sops-nix** today (or other deploy-secrets tool) |
| **2** | Human passwords, TOTP, notes, mail cred inventory UX | **Vaultwarden** (planned) |
| Disk | Disk at rest | **LUKS2** FDE first-class (not the same as 1 or 2) |

Full story: [SECRETS.md](SECRETS.md), [SECURITY.md](SECURITY.md).

```text
  host-local deploy secrets --activate--> /run/secrets/* --> services
                                              |
                                              +--> vaultwarden (needs deploy secrets to start)
                                              +--> stalwart, restic, ...

  humans <--> Vaultwarden UI/API   (not used at flake eval time)

  public git: ZERO secret material (plain or ciphertext)

  LUKS2 unlock (initrd: passphrase / initrd SSH / TPM)
       --> root FS --> then deploy decrypt --> services
  (sops-nix does NOT unlock LUKS; unlock material NEVER in git;
   see research/luks2-and-deploy-secrets.md, hygiene.md top rule)
```

## Request paths

### Mail (direct to Stalwart)

| Port | Protocol | Public? |
|------|----------|---------|
| 25 | SMTP inbound | Yes |
| 465 | SMTPS submission | Yes |
| 587 | Submission + STARTTLS | Yes |
| 993 | IMAPS | Yes |
| 4190 | ManageSieve | Yes |
| Stalwart HTTP (JMAP + webadmin) | HTTP | **No** (UDS preferred; loopback TCP scaffold) |

Firewall set in `modules/networking.nix`.

### HTTPS (edge -> backends)

| Host | Path | Upstream |
|------|------|----------|
| `services.surmount.systems` | `/` | management-ui (UDS target; TCP loopback scaffold) |
| `services.surmount.systems` | `/stalwart-admin/` | Stalwart HTTP (bootstrap only) |
| `mail.surmount.systems` | `/` | redirect to services (ACME name for mail certs) |
| apex / www | `/` | park / redirect; static legacy later |
| future vault host | `/` | Vaultwarden local only (private, no public signup) |

JMAP clients should eventually hit either:

1. Edge path that proxies to Stalwart HTTP with proper auth, or
2. Management-ui JMAP proxy (`POST /api/v1/jmap`, currently 501 residual)

Prefer talking to Stalwart over JMAP/management APIs rather than reading
RocksDB ourselves.

### Static sites

Legacy content: **static files only**, no exceptions. Extra vhosts via
`surmount.web.extraVhosts` or future helpers. No PHP/Node for old sites.

## Topology: single host (ends when operator says)

- One mail VPS now (operator-chosen; size direction above).
- Modules stay readable/separable; no active multi-node design.
- Do not invent scale-out triggers.

## No Cloudflare (required path)

Traffic must work **direct to the VPS** (A/AAAA on the mail and services
names). We do not depend on orange-cloud proxy, CF Access, Workers, WAF, or
Tunnel as critical path. Registrar can be anywhere (including CF DNS-only).
Cloudflare **Research** blog posts on post-quantum TLS may be **cited for
learning**; that is not a product dependency.
See [EDGE_AND_TLS.md](EDGE_AND_TLS.md), [hygiene.md](hygiene.md),
[research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).

## Edge / TLS

- **Today (tree default):** **Axum-first** management-ui rustls
  (`listenMode=https` + host PEMs); `surmount.web.enable` default false.
  Host public cutover residual (RESIDUAL.md).
- **Dual-run escape:** nginx + `security.acme` when `web.enable = true`
  (**transitional-to-delete**). Separate proxy products only if measured need.
  **Not** Caddy/nginx as preferred identity.
- **Local hops:** Unix domain sockets preferred over TCP localhost.
- **TLS trust:** not locked ACME-only; see
  [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md).
- **PQ:** rustls hybrid KEX **first-class**; PQConnect **separate** path
  (evaluate/plan; not TLS substitute alone).
- **Arti hidden services (REQUIRED):** onion HS reachability for Surmount
  services; first-class next to clearnet; not a clearnet edge replacement.
  [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md),
  [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7.
- Full evaluation: [EDGE_AND_TLS.md](EDGE_AND_TLS.md)

## Product languages

**Nix + Rust** only for product deps. No Python, no NPM/Node product trees.
Gaps filled in-house. [principles.md](principles.md) section 5b.

## Mail earn-trust DNS

SPF, DKIM, DMARC, PTR/rDNS, MTA-STS, TLS-RPT; DNSSEC + DANE/TLSA planned.
Checklist: [DNS.md](DNS.md).

## Frontend / client direction

| Layer | Choice | Role |
|-------|--------|------|
| HTTP backend / shell | **Axum** | APIs, Nostr auth, health |
| Web UI framework | **Leptos SSR** | Admin first, then webmail v1 |
| Admin shell | Axum + Leptos SSR (ssr-only scaffold) | Hydrate/webmail residual |
| Product auth | **Nostr** keys | keys via host OS / Surmount tools |
| v1 browser mail | **Real webmail** after admin | JMAP via our app |
| Longer-term clients | Desktop/local-first + any standard MUA | Complementary, not a cancel of webmail |

Detail and phases: [SEARCH_AND_UI.md](SEARCH_AND_UI.md).

## Search split (summary)

1. **Mail search now:** Stalwart **internal FTS** + JMAP `Email/query`. Good
   enough for now. UI calls that; no parallel full-text mail index.
2. **Later:** Surmount builds its **own search product**. Until then do not
   add ES/Meilisearch for convenience.
3. **Surmount indexes:** only for product needs outside the mailbox
   (structured logs, audit, billing metadata, etc.).

## Self-ops posture

The stack should help the operator troubleshoot itself: journald + retention,
health endpoints, flake apps / `scripts/` for dig/openssl/mail checks,
fail2ban (or equivalent) without a third-party WAF. See [OPS.md](OPS.md)
and [hygiene.md](hygiene.md).

## Related docs

| Doc | Topic |
|-----|-------|
| [COMPACTION-PIN.md](COMPACTION-PIN.md) | Single reload file after compaction |
| [operator-direction.md](operator-direction.md) | Dated operator direction (2026-07-30) |
| [principles.md](principles.md) | Engineering principles |
| [glossary.md](glossary.md) | FOD, Day-1/Day-2, Fix, DataStore, ... |
| [packages-and-forks.md](packages-and-forks.md) | Overlay, fork consume, no agent git |
| [open-choices.md](open-choices.md) | Still open / proposed |
| [DATASTORES.md](DATASTORES.md) | Data plane inventory + Stalwart storage |
| [SEARCH_AND_UI.md](SEARCH_AND_UI.md) | Search + UI phases |
| [EDGE_AND_TLS.md](EDGE_AND_TLS.md) | Edge, TLS, rate limits, Axum-first; no nginx target |
| [SECURITY.md](SECURITY.md) | FDE, secrets layers, threat model, Nostr |
| [SECRETS.md](SECRETS.md) | Full secrets architecture |
| [OPS.md](OPS.md) | Logs, scripts, runbooks posture |
| [hygiene.md](hygiene.md) | Standing engineering rules |
| [architecture-review.md](architecture-review.md) | Peer review of tree facts (partially superseded) |
| [DNS.md](DNS.md) | Zone / MX / earn-trust (SPF/DKIM/DMARC/PTR/MTA-STS/TLS-RPT/DANE) |
| [MIGRATION.md](MIGRATION.md) | MailPlus Maildir import |
| [fix-and-fixos.md](fix-and-fixos.md) | Fix / FixOS ladder |
| [../secrets/README.md](../secrets/README.md) | sops bootstrap |
| [research/](research/) | Deep / historical evidence notes |
| [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md) | TLS hybrid PQ + PQConnect; CF Research citations |
| [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md) | Arti HS **required** + VW vs Bitwarden Secrets Manager |
