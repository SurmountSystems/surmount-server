# Search, indexing, and UI plan

Implementation-oriented plan for mail search ownership and the management /
user-facing UI path. Design notes: [open-choices.md](open-choices.md).
Operator direction: [operator-direction.md](operator-direction.md).
Stack map: [STACK.md](STACK.md). Glossary: [glossary.md](glossary.md)
(Internal FTS).

**Last updated:** 2026-07-30

## Principles

1. **Stalwart owns the mailbox and its native index for now.** We do not fork a
   parallel mail store or full-text index of message bodies in this phase.
2. **Our Rust layer owns** the management console, **v1 webmail**, auth glue
   (Nostr), and product UX. Prefer **JMAP** and Stalwart management/HTTP APIs.
3. **Mail search UX** calls Stalwart/JMAP first. Surmount-side indexes only
   for non-mail product data (logs, audit, metadata) with a named need.
4. **UI depth order:** **admin console first**, then **real webmail in v1**
   (operator direction).
5. **Axum + Leptos SSR** is the invested web stack (operator direction).
   Embedded HTML is a bridge only.
6. **Product auth is Nostr** (keys via host OS / Surmount tools; operator
   direction). Stalwart still holds mail credentials; human tracking via
   Vaultwarden; bridge pattern in STACK + SECURITY.
7. Desktop/local-first clients remain a long-term preference **alongside**
   browser admin and v1 webmail, not instead of them on day one.
8. **Later:** Surmount builds its **own search product**. Until then internal
   FTS is **good enough for now**.

## What is internal FTS?

**Internal FTS** is Stalwart's built-in full-text search index in the search
store role. On our layout (Default search store) that index lives in the same
RocksDB as metadata. It is not Elasticsearch and not a separate cluster.
Clients search via JMAP `Email/query` or IMAP SEARCH. Surmount product code
does **not** open RocksDB for search.

| Now | Later |
|-----|-------|
| Use internal FTS + JMAP | Surmount-owned search product |
| No ES/Meilisearch for convenience | Revisit only with product direction + measurement |

## What Stalwart provides (search)

Official:

- [Search store (FTS)](https://stalw.art/docs/storage/fts/)
- [Storage overview](https://stalw.art/docs/storage/)
- [JMAP HTTP](https://stalw.art/docs/http/jmap/) (entry via docs site)
- JMAP mail model: [RFC 8621](https://datatracker.ietf.org/doc/html/rfc8621)
  (`Email/query`, filters, sort, etc.)

On our single-node layout (all stores co-located on RocksDB; **fine for now**;
[DATASTORES.md](DATASTORES.md); version from `nix/packages/stalwart-mail.nix`):

- Full-text index lives with the data store (internal FTS). Higher write amp
  than some dedicated engines; we have **not** load-tested quality/latency yet.
- That is still **good enough for now** (operator direction). Do not add
  Elasticsearch or Meilisearch for convenience.
- Clients search via JMAP (preferred for our product) or IMAP SEARCH
  (interop with existing MUAs).

Do **not** read RocksDB from Surmount code for search. Always go through
published protocols/APIs.

Historical note on FTS structures under old 0.11.8:
[research/stalwart-stores-evidence-2026-07-30.md](research/stalwart-stores-evidence-2026-07-30.md).

## What Surmount may index later

Only with a clear product reason:

| Data | Example need | Store ideas |
|------|--------------|-------------|
| Operator audit events | "who changed what" | Append-only log / SQLite / journal |
| App request metrics | rate / abuse for our UI | structured logs, not mail FTS |
| npub <-> account map | authz | small DB or config + sops as appropriate |
| Non-mail site metadata | inventory of static hosts | small DB owned by product |
| Desktop sync cursors | local-first device state | client-side + small server tokens |

**Out of scope by default:** second copy of email bodies for "our own"
Elasticsearch.

## UI architecture

```text
Browser (admin, then v1 webmail)          Desktop / MUA (always OK)
     |                                         |
     | Nostr sign (NIP-98) / session           | IMAP/SMTP app password
     v                                         v
 HTTPS edge ----> Axum shell <---- (same API contracts where shared)
                     |
                     +-- /api/v1/*  (JSON; Nostr or session auth)
                     +-- Leptos SSR pages (target)
                     |     admin/*  then  mail/* (webmail)
                     |
                     +-- JMAP proxy (future) ----> Stalwart :8081
                     +-- status / admin APIs ----> Stalwart :8081
```

Stalwart webadmin (`/admin` on loopback, optional `/stalwart-admin/` via edge)
is a **bootstrap fallback**, not the long-term Surmount UX.

Legacy public sites: **static files only** at the edge. Not part of
the Leptos app.

## Phases

### Phase 0 - Skeleton (current)

**Status:** largely done in-tree.

- Axum binary `surmount-management-ui`
- Routes: `/`, `/health`, `/api/v1/domains`, `/api/v1/accounts`,
  `/api/v1/stalwart/status`, `POST /api/v1/jmap` (501)
- Loopback bind; nginx proxies `services.surmount.systems` (transitional;
  product edge is Axum rustls when `web.enable = false`)
- Stalwart admin via SSH tunnel (preferred) or bootstrap path
- Nix package + systemd module + flake checks
- **Leptos SSR** admin shell on `GET /` (ssr-only scaffold; marker
  `data-surmount-ssr="leptos"`). No WASM hydrate / NPM yet.

**Exit criteria:** deployable console that proves the host is alive and can
reach Stalwart HTTP.

### Phase 1 - Admin API + Nostr auth foundation

- **Nostr-first auth** for operators: verify NIP-98 (and/or explicit
  challenge) against an allowlisted set of npubs; establish session for SSR
  (open design: signed cookie vs server session; see open-choices)
- Wire domains/accounts to Stalwart directory APIs or declarative inventory
  exported from Nix (be honest which is source of truth)
- Replace 501 JMAP proxy with authenticated forward suitable for admin
  tooling, or document dedicated JMAP vhost
- Structured logging on the Rust service (JSON or key=value to journald)
- Richer `/health` (deps: Stalwart reachability already partial)

**Exit criteria:** allowlisted operator can authenticate with Nostr and
list/create accounts without Stalwart webadmin for the happy path.

### Phase 2 - Leptos SSR admin UI

- **Scaffold done:** Leptos SSR home shell on Axum (`pages.rs`, ssr feature).
- Remaining: richer admin pages; hydrate only where needed; no cargo-leptos
  dual build required until interactive islands land
- Keep Axum as the server integration point (rate-limit, ban, rustls)
- Admin features: domain list, account status, queue/metrics if Stalwart
  exposes them, link to runbooks, issue/rotate app passwords for MUAs
- Still not full webmail (that is Phase 3)

**Exit criteria:** day-to-day admin no longer requires Stalwart webadmin for
common tasks; `/stalwart-admin/` can be disabled by default.

### Phase 3 - Real webmail in v1

- **In v1 scope** after admin is usable
- Read, search, compose, basic folder navigation via JMAP through our app
- Mail search uses JMAP `Email/query` + Stalwart FTS; snippets if available
- Same Nostr (or delegated user) auth story; map identity to mail account
- Hardening: CSRF, CSP, attachment handling, session fixation; SECURITY.md
- No Surmount-side parallel mail FTS

**Exit criteria:** user can do core mail tasks in the browser without a
second index; quality is "real webmail," not a toy iframe of Stalwart.

### Phase 4 - Onboarding + desktop pointer UX

- Public/light web: status, docs links, **download Surmount desktop** when
  it exists
- Onboarding: DNS checklist, account created, client download, app password
- Optional: deep links that open local app where installed
- Webmail remains available; desktop is complementary

**Exit criteria:** browser covers admin + webmail + onboarding; desktop is
an encouraged path, not a gate.

### Phase 5 - Optional Surmount indexes (only if needed)

- Add product DBs for audit/logs/metadata with migrations and backup story
- Never silently become "we also store all mail here"
- Document each new store in STACK + DATASTORES inventory

## API sketch (stable enough to build toward)

| Method | Path | Intent |
|--------|------|--------|
| GET | `/health` | Liveness for edge/monitor |
| GET | `/api/v1/stalwart/status` | Best-effort upstream probe |
| GET | `/api/v1/domains` | Inventory (Nix and/or Stalwart) |
| GET/POST | `/api/v1/accounts` | Directory operations |
| POST | `/api/v1/jmap` | Authenticated JMAP proxy (or 501 until ready) |
| (TBD) | `/api/v1/auth/*` | Nostr challenge / session exchange |

Exact auth headers and error shapes: define in crate docs when Phase 1 lands;
do not invent parallel mail REST that reimplements JMAP.

NIP-98 reminder: clients send `Authorization: Nostr <base64(kind 27235 event)>`
with `u` = absolute URL and `method` = HTTP method; server checks signature,
time window, URL, method
([NIP-98](https://nips.nostr.com/98)).

## Explicit non-goals (until living docs change them)

- Forking Stalwart
- Replacing JMAP with a custom mail REST as the only API
- Running Elasticsearch "because search"
- Claiming Stalwart natively speaks Nostr
- Admin-only forever (webmail is v1)
- Webmail before a usable admin path
- Requiring Cloudflare Access in front of the admin UI
- Multi-host UI/session topology (deferred)
- Dynamic app servers for legacy Synology sites (static only)

## Testing expectations

- Unit/integration tests in `crates/management-ui` for API contracts and
  Nostr auth verification (fixtures with known keys)
- NixOS VM smoke: UI + Stalwart up (`tests/mail.nix`)
- Red/green for behavior changes (project hygiene)

## Related code

| Path | Role |
|------|------|
| `crates/management-ui/` | Axum app + Leptos SSR admin shell (ssr-only scaffold) |
| `modules/management-ui.nix` | systemd + env |
| `modules/mail.nix` | Stalwart |
| `modules/web.nix` | edge vhosts (transitional nginx; static sites) |
