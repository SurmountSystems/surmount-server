# Open choices and working notes

Plain English design notes for Surmount Server. **Operator direction** dated
2026-07-30 (plus same-day follow-up answers) lives in
[operator-direction.md](operator-direction.md) and is stronger than scaffold
defaults when they conflict. Items below are either still **open**,
**proposed**, **scaffold default**, or **research finding** unless that
direction file already settled them.

**Last updated:** 2026-07-30

**Peer review of current tree (versions, stores, assumptions):**
[architecture-review.md](architecture-review.md) (see supersession banner for
host sizing and RocksDB OK-for-now). Fix/FixOS ladder direction is open
working direction in [fix-and-fixos.md](fix-and-fixos.md).

Living maps: [STACK.md](STACK.md), [DATASTORES.md](DATASTORES.md),
[SEARCH_AND_UI.md](SEARCH_AND_UI.md), [SECURITY.md](SECURITY.md),
[SECRETS.md](SECRETS.md), [hygiene.md](hygiene.md),
[principles.md](principles.md), [glossary.md](glossary.md),
[packages-and-forks.md](packages-and-forks.md).
Deep research: [research/](research/).

Open questions use **unique global ids** in this file (namespaced where
helpful: **Q-HOST-1**, **Q-TLS-1**, ...).

---

## Directed (see operator-direction.md; not re-litigated here)

Summaries only. Detail and wording of record: operator-direction.md.

| Topic | Direction (2026-07-30 + follow-up) |
|-------|-------------------------------------|
| Host | Operator-chosen VPS; size/plan open (Q-HOST-1); do not invent provider name or RAM/disk/core/SKU numbers; assume NixOS allowed |
| RocksDB | Fine for now; all-role co-location OK |
| Internal FTS | Good enough for now; Surmount own search product later |
| In-memory store | Fine on RocksDB for now |
| Blob tiering / compliance split | Not now |
| Clever RPO/RTO | Later, not now |
| Edge | No nginx product edge; **Axum-first** HTTPS edge preferred; UDS local hops; separate proxy products only if measured need |
| Auth | Nostr keys; simple; OS/Surmount tool key management |
| Human secrets | Vaultwarden; mail creds tracked there for humans |
| Deploy secrets | Encrypted secrets available at NixOS activation still required (sops-nix scaffold; need real; tool can change) |
| Disk | LUKS2 first-class (disk encryption; not deploy secrets or VW) |
| UI | Axum + Leptos SSR; admin then webmail |
| Legacy sites | Static only, no exceptions |
| Spam / security | First-class; integrity highest |
| Single VPS end | When operator says |
| Stalwart fork | SurmountSystems/stalwart; **flake input** integration; agents never git fork unless instructed |
| poolWorkers | Leave upstream default (logical CPU count on the operator-chosen host) |
| Cert / ACME lock | **Not** locked ACME-only or rustls-acme-only; research open |
| **Arti onion / HS** | **REQUIRED** reachability via Arti hidden services alongside clearnet; not optional; not clearnet edge replacement |

---

## Fix / FixOS ownership ladder

**Open working direction** (not every package pin accepted as product law):
Surmount may need to own packaging and, later, more of the Nix/NixOS lineage.

- **Fix** - Surmount take / fork lineage of upstream **Nix**
- **FixOS** - Surmount take / fork lineage of upstream **NixOS**
- Etymology: **Facta Non Verba** ("deeds, not words")

Honest ladder and plain packaging names (upstream nixpkgs, Surmount package
overlay, future fixpkgs): [fix-and-fixos.md](fix-and-fixos.md),
[packages-and-forks.md](packages-and-forks.md),
[research/fix-fixos-ladder.md](research/fix-fixos-ladder.md).

Today: upstream nixpkgs + **Surmount package overlay** in this repo (Stalwart
FODs, modules, management-ui). Future fixpkgs channel not started as a
separate product.

---

## Engine version

**Stalwart is on current via Surmount package** (`nix/packages/stalwart-mail.nix`;
measured **0.16.15** binary FOD as of architecture-review). Compatibility with
old Stalwart is **not** a goal. nixpkgs lag is **not** a reason to stay old.

- Always re-validate latest when bumping.
- Historical note: early scaffold briefly used channel **0.11.8** on host
  **nixos-25.05**; living host is **nixos-26.05** and engine is Surmount FOD
  **0.16.15**.
- Evidence: [research/stalwart-0.16.15-stores-evidence.md](research/stalwart-0.16.15-stores-evidence.md).

**Still open:** binary FOD until source build, vs require source build before
live MX; link `pkgs.rocksdb` on source path (design goal).

---

## Mail engine

**Directed / proposed:** Stalwart remains the mail engine. SMTP, submission,
IMAP, ManageSieve, JMAP, spam filtering, and the authoritative message store
live there. Surmount does not reimplement an MTA or keep a parallel mailbox
corpus.

- Wiring: Surmount `modules/stalwart-service.nix` + `modules/mail.nix`.
- Migration: Maildir/MBOX import ([MIGRATION.md](MIGRATION.md)); priority
  recover Synology MailPlus data.
- Product UI talks over HTTP/JMAP/admin APIs and CLI helpers.

---

## Storage model

**Detail:** [DATASTORES.md](DATASTORES.md).
**Direction:** RocksDB co-location **fine for now**.

**Logical model:** Stalwart four store roles (data, blob, search/FTS,
in-memory/lookup). Internal directory in the data store. Product code does not
open engine files; JMAP/admin/CLI only.

**Physical now:** one RocksDB at `/var/lib/stalwart-mail/db`.

**Knobs:** host-agnostic starting points for blobSize / bufferSize /
poolWorkers in operator-direction.md section 2. **poolWorkers:** leave
upstream default (logical CPU count on the real host).

**Still open later (not Day-1 blockers):** Postgres for data; split blobs;
Surmount search product replacing reliance on internal FTS; product SQLite
under `/var/lib/surmount`.

Do not add PostgreSQL, FoundationDB, S3, Elasticsearch, Meilisearch, or Redis
until measurement or new operator direction, and living docs update same turn.

---

## Search ownership

**Now (directed):** Stalwart **internal FTS** is good enough. Mail search in
our UI uses Stalwart/JMAP first. No parallel mailbox FTS for convenience.

**Later (directed):** Surmount builds its **own search product**.

Detail: [SEARCH_AND_UI.md](SEARCH_AND_UI.md), glossary "Internal FTS".

---

## Web stack

**Directed:** Axum as HTTP shell; **Leptos SSR** as the invested UI path.
Embedded HTML is a bridge only. No primary React/Vue SPA.

---

## Admin first, then webmail in v1

**Directed:** operator admin console first, then real webmail in v1 via JMAP.
Stalwart `/admin` is bootstrap fallback only. Desktop/local-first clients stay
complementary later, not a cancel of browser admin or v1 webmail.

---

## No required Cloudflare hop

**Directed / proposed:** traffic works direct to the VPS (A/AAAA). No required
orange-cloud proxy, CF Access, Workers, WAF, or Tunnel for mail or core
services. Registrar may still be Cloudflare DNS-only.

---

## Edge / TLS

**Today (tree default):** **Axum-first** management-ui rustls
(`listenMode=https` + host PEMs); `surmount.web.enable` default **false**.
Host public cutover + MDWE smoke still residual (RESIDUAL.md).

**Dual-run escape:** `surmount.web.enable = true` + UI http loopback; nginx +
`security.acme` in `modules/web.nix` (**transitional-to-delete**).

**Directed:** TLS, certs, rate limits, routing owned in our stack. Prefer
**Unix domain sockets** to local backends. Separate reverse-proxy products
(Caddy, Sozu, etc.) only if measured need. nginx module file remains until
dual-run is unused.

Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md),
[research/rust-edge-and-uds.md](research/rust-edge-and-uds.md).

**TLS trust (research, not locked):** public CA + ACME is common; not the only
path. Mail TLS vs browser HTTPS may differ.
[research/tls-trust-and-acme.md](research/tls-trust-and-acme.md).

**TLS posture (operator direction lean):** :80 -> :443; no SSLv3/1.0/1.1;
prefer TLS 1.3; PQ hybrid KEX where rustls supports; open CA list (LE,
ZeroSSL, Buypass, GTS, commercial ACME). No third-party reverse proxy product.

**PQC / PQConnect (research):** E2EE PQC first-class; **TLS hybrid KEX
first-class** on rustls; PQConnect **separate** path; not in nixpkgs (latest
upstream release 1.2.1 as of 2026-07-30). CF Research PQ posts cited for
learning only (no CF products).
[research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).

**Arti / secrets manager:** Tor Project **Arti** onion/hidden services are
**REQUIRED** (operator direction 2026-07-30). First-class next to clearnet.
Vaultwarden is **not** Bitwarden Secrets Manager API today (SM API gap).
[research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md),
[COMPACTION-PIN.md](COMPACTION-PIN.md) section 7.

**Access control (operator direction lean):** merciless ban on unauthorized
access; whitelist never banned; last-used tracking; rate limits; prefer Rust
+ nft long-term over Python fail2ban as identity.
[research/access-control-fail2ban.md](research/access-control-fail2ban.md).

**Product languages (directed lean):** Nix + Rust only; no Python/NPM product
deps ([principles.md](principles.md) section 5b).

**Mail earn-trust DNS:** checklist in [DNS.md](DNS.md); DANE timing still open
(**Q-TLS-2**).

**Still open:**

**Q-EDGE-1.** Shared host ACME PEM files for mail + web vs in-process issuance
on the Axum edge for web only?

**Q-EDGE-2.** UDS path layout (`/run/surmount/*.sock` vs per-service `/run/...`)?

**Q-TLS-1** ... cert trust / DANE / short-lived / multi-CA: see tls-trust-and-acme.md.

**Q-CA-1.** Primary public CA at cutover?

**Q-CA-2.** Dual-ACME failover on a single VPS?

**Q-PQC-1.** PQConnect Day-1 vs Day-2?

**Q-PQC-2.** Who runs PQConnect client?

**Q-PQC-3.** Mail-plane PQ path (Stalwart stack vs PQConnect vs DANE mix)?

**Q-PQC-4.** Package PQConnect overlay now or wait?

**Q-ARTI-1.** ~~Enable Arti at all?~~ **Answered:** **required** for onion/HS
reachability (operator 2026-07-30). Implementation residual; not optional.

**Q-ARTI-2.** First onion surfaces: private HTTP (UI/VW), selected admin,
mail protocols, or phased set?

**Q-ARTI-3.** Onion exposure of Stalwart admin/JMAP? Default lean no.

**Q-ARTI-4.** Provider ToS / abuse if non-exit relay shares MX IP? (Relay
optional; HS required.)

**Q-SEC-SM-1.** Hard-require Bitwarden Secrets Manager API, or deploy secrets +
VW password manager (+ optional `bw` CLI) enough?

**Q-SEC-SM-2.** If true SM API required self-host: wait VW, commercial BW path,
or Surmount-owned Rust secrets service?

**Q-ACL-1.** Exact "unauthorized" signals for immediate ban?

**Q-ACL-2.** Whitelist Nix-only vs mutable + last-used store?

**Q-ACL-3.** Ban duration default?

**Q-ACL-4.** Rust `surmount-guard` timing vs keep fail2ban through mail live?

**Q-ACL-5.** IPv6 ban granularity?

**Q-ACL-6.** Mail AUTH failures on global blacklist or Stalwart-local only?

---

## Secrets (two buckets + optional disk)

**Directed split:**

| Bucket | Tool | Job |
|--------|------|-----|
| 1. Deploy secrets | sops-nix today (need required; tool open) | Decrypt on host at activation; **never in public git** |
| 2. Humans | Vaultwarden (planned) | Passwords, TOTP, notes, mail cred UX |
| Disk | LUKS2 first-class | Offline disk / snapshot; unlock material never in git |

Vaultwarden does not replace deploy secrets. Deploy secrets do not replace VW.
LUKS is disk encryption, not either bucket. Prefer **deploy secrets** language
over vague "seal." **NEVER secrets in git** ([hygiene.md](hygiene.md)).

**Fact (research):** sops-nix does **not** unlock or configure LUKS2. Initrd
unlock is separate; activation is too late for root FDE. Git is never the
unlock channel.
[research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

**Still open:**

**Q-DEP-1.** Keep sops-nix long-term, or evaluate an alternative deploy-secrets
tool (without dual-stacking agenix)? The *need* for secrets at activation
stays either way. Git storage stays forbidden.

**Q-LUKS-1.** LUKS from day one vs interim plain (with **Q-HOST-2**)?

**Q-LUKS-2.** Human initrd unlock vs weaker **host-local** keyfile-on-boot for
unattended reboot? (Keyfile never from git.)

**Q-LUKS-3.** Custom initrd age decrypt later? Default lean: no. Git storage
of unlock material remains forbidden either way.

**Q-LUKS-4.** Full root vs data-partition-only encryption?

Detail: [SECRETS.md](SECRETS.md), [SECURITY.md](SECURITY.md).

---

## Hermetic flake builds

**Proposed / principle:** locked `flake.lock`; no impure network at eval except
hashed FODs; `ref/` study-only; crane packages under `nix/packages/`.
Prefer system libs with fixed ABI on source builds (RocksDB design goal).
See [principles.md](principles.md).

---

## Migration is operator-run

**Proposed:** Maildir import via runbook/helpers only. No silent import in
`system.activationScripts`. See [MIGRATION.md](MIGRATION.md). Priority:
recover MailPlus onto self-sovereign Surmount Server.

---

## Self-ops

**Proposed direction:** journald + retention; health endpoints; `scripts/` for
DNS/TLS/mail checks; abuse controls carefully; runbooks in `docs/`. No
dependence on a third-party WAF for basic hygiene. First-class spam detection
at the mail engine. Merciless ban/whitelist is product direction (research
sketch); fail2ban remains light scaffold until Rust/nft path lands.

Detail: [OPS.md](OPS.md),
[research/access-control-fail2ban.md](research/access-control-fail2ban.md).

---

## Nostr-first product auth

**Directed:** operators (later users) auth to Surmount with Nostr keys. Dead
simple. Keys managed by host OS via other Surmount tools. Stalwart still holds
mail account credentials. Surmount bridges Nostr identity to Stalwart APIs.

**Still open:**

**Q-AUTH-1.** Session store shape, first-operator bootstrap allowlist, key-loss
recovery procedure?

Detail: [SECURITY.md](SECURITY.md), [SEARCH_AND_UI.md](SEARCH_AND_UI.md).

---

## Multi-host deferred

**Operator-directed:** one mail VPS for now. Ends **when the operator says**.
Do not invent triggers. Modules stay separable for clarity.

---

## Legacy sites are static only

**Directed:** old Synology content is static files at the edge. **No
exceptions.** No PHP/app servers for legacy sites. New product apps are
separate services.

---

## Host provider and disk encryption

**Directed shape:** operator-chosen VPS. **Do not invent or publish provider
names, RAM, disk, core counts, or plan SKUs.** Operator will ask the provider
about NixOS + LUKS2. Assume NixOS is allowed. `poolWorkers` defaults to
logical CPU count on whatever host is bought (leave default unless measured
need to cap).

**Still open:**

**Q-HOST-1.** Exact provider and plan SKU? PTR control confirmed?

**Q-HOST-2.** First install LUKS from day one, or interim plain disk then
reinstall (after provider answers)?

Detail: [SECURITY.md](SECURITY.md), operator-direction.md.

---

## Stalwart fork consume path

**Directed:** use SurmountSystems/stalwart when patches needed; integrate as
a **flake input**; agents never git the fork unless instructed. Not a forever
side manual build outside the flake.

Detail: [packages-and-forks.md](packages-and-forks.md).

---

## RocksDB poolWorkers in Nix

**Directed for this host shape:** leave upstream default (logical CPU count;
16 on the directed box). No requirement to pin in Nix Day-1. Add a Nix option
later only if we choose to pin or cap.

---

## Data-plane inventory process

**Proposed process:** [DATASTORES.md](DATASTORES.md) holds the living inventory
of durable stores. New stores land in that table in the same change as code,
with engine, path, durability, backup, secrets, and owner filled in.

---

## Doc layout

| Path | Role |
|------|------|
| `docs/COMPACTION-PIN.md` | Single reload file after compaction |
| `docs/operator-direction.md` | Dated operator direction dump + follow-up answers |
| `docs/principles.md` | Standing engineering principles |
| `docs/glossary.md` | Terms (FOD, Day-1, Day-2, deploy secrets, ...) |
| `docs/packages-and-forks.md` | Overlay, fork consume, agent git rules |
| `docs/architecture-review.md` | Peer review: versions, stores, assumptions |
| `docs/open-choices.md` | This file: still open / proposed |
| `docs/research/` | Deep evidence notes (version-pinned, historical when labeled) |
| `docs/research/tls-trust-and-acme.md` | Cert trust / ACME options (not locked) |
| `docs/research/pqconnect-and-pqc.md` | TLS hybrid PQ + PQConnect; industry PQ research cites |
| `docs/research/arti-and-secrets-manager.md` | Arti + VW vs Bitwarden Secrets Manager |
| `docs/DNS.md` | Earn-trust mail DNS checklist |
| `AGENTS.md` | Agent process pins |
