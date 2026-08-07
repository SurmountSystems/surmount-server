# Search, indexing, and UI plan

Implementation-oriented plan for mail search ownership and the management /
user-facing UI path. Design notes: [open-choices.md](open-choices.md).
Operator direction: [operator-direction.md](operator-direction.md).
Stack map: [STACK.md](STACK.md). Glossary: [glossary.md](glossary.md)
(Internal FTS).

**Last updated:** 2026-08-07

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

### Phase 0 - Foundation console (current)

**Status:** multi-page operator console in-tree (no skeleton cosplay).

- Axum binary `surmount-management-ui`
- HTML routes (SSR): `/`, `/domains`, `/accounts`, `/system`, `/mail`
- JSON: `/health`, `/api/v1/system` (optional onion), `/api/v1/domains`
  (config inventory), `/api/v1/accounts` (directory strategy: default
  honest empty / `source: unavailable`; hermetic `mock`; live `stalwart`
  via management JMAP when explicitly configured + host token),
  `/api/v1/stalwart/status` (live probe), `POST /api/v1/jmap` (honest 501
  boundary: `jmap_proxy_not_implemented`; full proxy + webmail residual)
- Onion display when `SURMOUNT_ONION_URL` or hostname file set (never invented)
- Directory trait + hermetic mock + live Stalwart client shipped (explicit
  opt-in only; never default-on; fail-closed misconfig):
  [research/stalwart-directory-api.md](research/stalwart-directory-api.md)
- Loopback bind; nginx proxies `services.surmount.systems` (transitional;
  product edge is Axum rustls when `web.enable = false`)
- Stalwart admin via SSH tunnel (preferred) or bootstrap path
- Nix package + systemd module + flake checks
- **Leptos SSR** multi-page admin console (ssr-only; marker
  `data-surmount-ssr="leptos"`; DOGE palette). No WASM hydrate / NPM yet.

**Exit criteria:** deployable console that proves the host is alive and can
reach Stalwart HTTP.

### Phase 1 - Admin API + Nostr auth foundation

**Progress (2026-08-01):** Nostr auth **foundation shipped** (not full Q-AUTH-1).

- **Library:** rust-nostr (`nostr` 0.43 + `nip98` feature). **Not** JS NDK /
  `@nostr-dev-kit/ndk` (no NPM product stack).
- **Modes:** `SURMOUNT_AUTH_MODE=off` (default, open console for `just dev`)
  or `nostr` (gate admin HTML + JSON APIs).
- **Allowlist:** `SURMOUNT_NOSTR_ALLOWLIST` (npub or hex) and optional
  `SURMOUNT_NOSTR_ALLOWLIST_FILE` (same parse; **env wins** when non-empty).
  Empty + mode=nostr = **fail-closed**. Scaffold bootstrap only;
  first-operator UX still Q-AUTH-1.
- **NIP-98:** kind 27235 verify (signature, `u`, `method`, skew window default
  300s via `SURMOUNT_NIP98_MAX_SKEW_SECS`). Session exchange
  `POST /api/v1/auth/session`; optional NIP-98 on every protected request.
- **Session:** HMAC-signed HttpOnly cookie (`SURMOUNT_SESSION_SECRET` required
  when mode=nostr). Scaffold, not final durable session store (Q-AUTH-1).
- **Routes:** `GET /api/v1/auth/challenge`, `POST /api/v1/auth/session`,
  `POST /api/v1/auth/logout`, `GET /api/v1/auth/me`, `GET /login` (optional
  vanilla NIP-07 script; no NPM). Public always: `/health`, auth endpoints.
- **Local enable mini-runbook:** export `SURMOUNT_AUTH_MODE=nostr`,
  `SURMOUNT_NOSTR_ALLOWLIST=npub1...` (or hex), and
  `SURMOUNT_SESSION_SECRET=$(openssl rand -hex 32)`, then `just dev`. Open
  `/login`. Full table: [OPS.md](OPS.md) section *Local Nostr auth enable*.
  Local green != cutover; nsec never on server.
- **Still residual:** key-loss recovery; durable server session store choice;
  first-operator bootstrap product UX; JMAP authenticated forward; full
  webmail CSP. Live directory **list** + **create/update mutations** shipped
  (explicit `SURMOUNT_DIRECTORY=stalwart` or mock; requires authMode=nostr
  unless lab escape; CSRF on cookie POSTs). Structured request logging shipped
  (onion-redacted; no secret headers).

**Exit criteria (full Phase 1):** allowlisted operator authenticates with
Nostr and list/create accounts without Stalwart webadmin for the happy path
(**list + lean API create/update shipped**; password/credential product UX
and full Q-AUTH-1 still residual).

### Phase 2 - Leptos SSR admin UI

- **Multi-page console done (2026-08-01):** overview, domains, accounts,
  system, mail on Axum (`pages.rs`, ssr feature); shared DOGE chrome;
  honest empty accounts; config domain inventory; live Stalwart probe.
- **Live Stalwart directory list client shipped (2026-08-07):** trait + mock +
  management JMAP client; default still honest empty; never default-on.
- Remaining: hydrate only where needed; no cargo-leptos dual build until
  islands land; richer queue/metrics if Stalwart exposes them; link to
  runbooks; issue/rotate app passwords for MUAs (account create/update API
  lean surface already shipped; no HTML form)
- Keep Axum as the server integration point (rate-limit, ban, rustls)
- Still not full webmail (that is Phase 3)
- Auth foundation shipped (mode off default); full Q-AUTH-1 residual remains

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
| POST | `/api/v1/jmap` | Honest 501 today (`jmap_proxy_not_implemented`); authenticated JMAP proxy residual |
| GET | `/api/v1/auth/challenge` | Absolute `u` URL, method, skew, kind 27235 |
| POST | `/api/v1/auth/session` | NIP-98 exchange -> Set-Cookie session |
| POST | `/api/v1/auth/logout` | Clear session cookie |
| GET | `/api/v1/auth/me` | 200 + npub when session/NIP-98 valid; else 401 |
| GET | `/login` | SSR login help + optional NIP-07 (vanilla JS) |

Auth headers: `Authorization: Nostr <base64(kind 27235 event JSON)>` and/or
JSON body `{ "event": { ... } }` / base64 string. Session cookie
`surmount_session` (HMAC scaffold). Do not invent parallel mail REST that
reimplements JMAP.

NIP-98: clients send `Authorization: Nostr <base64(kind 27235 event)>` with
`u` = absolute URL and `method` = HTTP method; server checks signature,
time window (`SURMOUNT_NIP98_MAX_SKEW_SECS`, default 300), URL, method
([NIP-98](https://nips.nostr.com/98)). Implementation: **rust-nostr**, not JS NDK.

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
| `crates/management-ui/` | Axum app + Leptos SSR multi-page management console (ssr-only) |
| `modules/management-ui.nix` | systemd + env |
| `modules/mail.nix` | Stalwart |
| `modules/web.nix` | edge vhosts (transitional nginx; static sites) |
