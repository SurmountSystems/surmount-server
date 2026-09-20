# Search, indexing, and UI plan

Implementation-oriented plan for mail search ownership and the management /
user-facing UI path. Design notes: [open-choices.md](open-choices.md).
Operator direction: [operator-direction.md](operator-direction.md).
Stack map: [STACK.md](STACK.md). Glossary: [glossary.md](glossary.md)
(Internal FTS).

**Last updated:** 2026-09-07 (`/domains` and `GET /api/v1/domains` are a
config union of primary names including derived www and mta-sts when
those are first-class, every `static_vhosts` key, and extra mail
hostnames such as `mail.cryptoquick.com`. `source` stays `config`. That
table is not a three-name inventory, not Namecheap API, and not Stalwart
directory.) Prior 2026-09-03 (HTTP/3 NIP-07 session POST must not 500 for
missing Axum `ConnectInfo`; NWC is not that login). Prior 2026-08-25 (living mailbox map stays in
`~/.agents/surmount-server/operator-facts.md`, not this public file.)
Prior 2026-08-24 (`just deploy` publishes static sites; `just deploy-host` is the NixOS generation.) Prior 2026-08-21 (living mailbox map:
`~/.agents/surmount-server/operator-facts.md`. Do not assume Thunderbird.
Prior 2026-09-11: each public HTTP Host has its own v3 onion.
Prior 2026-08-20: Onion-Location + Alt-Svc on every public Host)

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
                     +-- JMAP proxy (future) ----> Stalwart :8080
                     +-- status / admin APIs ----> Stalwart :8080
```

Stalwart webadmin (`/admin` on loopback, optional `/stalwart-admin/` via edge)
is a **bootstrap fallback**, not the long-term Surmount UX.

Legacy public sites: **static files only** at the edge. Not part of
the Leptos app. Live apex/www serve the packaged
`SurmountSystems/site` tree (`pkgs.surmount-public-site`; Host
`surmount.systems` / `www.surmount.systems`). `just deploy` publishes
static sites; `just deploy-host` is the NixOS generation.
Operator console stays on `services.surmount.systems`. COMING SOON
leftover is closed.

## Phases

### Phase 0 - Foundation console (current)

**Status:** multi-page operator console in-tree (no skeleton cosplay).

- Axum binary `surmount-management-ui`
- HTML routes (SSR): `/`, `/domains`, `/accounts`, `/system`, `/mail`
- JSON: `/health`, `/api/v1/system` (onion surface + `onion_discovery` map), `/api/v1/domains`
  (config union: primary names including derived `www.{primary}` and
  `mta-sts.{primary}` when those are first-class, every `static_vhosts`
  key (six extra static zones plus cryptoquick apex/www), and extra mail
  hostnames such as `mail.cryptoquick.com`; `source` stays `config`; not
  Namecheap API; not Stalwart directory), `/api/v1/accounts` (directory strategy: default
  honest empty / `source: unavailable`; hermetic `mock`; live `stalwart`
  via management JMAP when explicitly configured + host token),
  `/api/v1/stalwart/status` (live probe), `POST /api/v1/jmap` (honest 501
  boundary: `jmap_proxy_not_implemented`; full proxy + webmail residual)
- Onion status: real host path via `surmount.artiHiddenService` (hostname under
  `onionServiceStateDir` / derived `SURMOUNT_ONION_HOSTNAME_FILE`); structured
  `configured` / `hostname_missing` / `not_provisioned` on system +
  `GET /api/v1/system`. Lab override `SURMOUNT_ONION_URL` only; never invent.
  **Discovery headers (2026-08-17, every public Host 2026-08-20;
  per-site v3 2026-09-11):**
  Onion-Location + Alt-Svc on mapped HTTPS 2xx/3xx. Map: apex, www,
  services, `mta-sts.{apex}`, extra static Hosts; one v3 per Host.
  Onion-Location is `{that-host-onion}{path}`. Mail unmapped. Mapping loaded
  at process start; restart the unit after hostname/env/map/static-vhost
  change (no hot-reload). Optional env: `SURMOUNT_ONION_LOCATION_ENABLED`,
  `SURMOUNT_ONION_ALT_SVC_ENABLED`, `SURMOUNT_ONION_LOCATION_DISABLED_HOSTS`,
  `SURMOUNT_ONION_ALT_SVC_DISABLED_HOSTS`, `SURMOUNT_ONION_MAP_FILE`.
  `GET /api/v1/system` dumps `onion_discovery` (admin-gated when Nostr on).
  Not on `/health`. `/vault` on services (proxy on) is path-preserving and
  not Nostr-gated (VW login is SoT). Live Arti unit is active; Tor Browser
  verify remains residual. Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md).
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
  **HTTP/3 (2026-09-03):** browsers that take Alt-Svc `h3` POST session
  without Axum `ConnectInfo`. That must be JSON 401/200, not 500 missing
  extension. NWC is a separate `/mail` wallet store; it is not this login.
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
(**list + lean API create/update shipped**; **Create mailbox + mailbox
password + attach npub (Grant console login) shipped** on `/mail`; optional
npub + Administrator/User map; attach does not need directory listing;
full Q-AUTH-1 still residual).

### Phase 2 - Leptos SSR admin UI

- **Multi-page console done (2026-08-01):** overview, domains, accounts,
  system, mail on Axum (`pages.rs`, ssr feature); shared DOGE chrome;
  honest empty accounts; `/domains` config union (primary names plus
  derived www and mta-sts, `static_vhosts` keys, extra mail hostnames;
  `source` stays `config`); live Stalwart probe.
- **Live Stalwart directory list client shipped (2026-08-07):** trait + mock +
  management JMAP client; default still honest empty; never default-on.
- **Operator UX pass (2026-08-10):** Overview is a health-first operator
  dashboard, not a residual diary. Product language on Overview (no
  `SURMOUNT_*` primary residual for auth/directory/onion/vault). System holds
  technical detail and env wiring. DOGE palette kept; craft pass on type
  scale, card chrome, chip semantics (ok / fail / warn / neutral).
- **Synology-style package home (2026-08-10):** Overview is a DSM-like package
  console: one-line health strip, large package tiles (Console, Mail, Domains,
  Accounts, Onion, Vault), and a compact alert bar of short chips only when
  residual exists. No tutorial lede, Hostnames card, Navigate wall, or
  per-service residual essays on home. Detail and env names live on **System**.
  Hermetic contract: `overview_operator_language_no_env_primary_residual`.
  Follow-on backlog:
  [`.agents/reports/ux-residual-follow-on.md`](../.agents/reports/ux-residual-follow-on.md).
- **Mailbox password form shipped (2026-08-14):** `/mail` leads with the IMAP
  password card (Evolution and iPhone Mail; not Nostr). Card fragment stays
  `#mailbox-password`; the password INPUT id is `mailbox-password-input`
  (unique; the form script reads that input). `POST /api/v1/accounts/password`
  looks up the principal by email and PATCHes a Password credential on that
  Account (not the API-token `AccountPassword` singleton). The live client
  POSTs management JMAP to `SURMOUNT_STALWART_URL/jmap` (Stalwart 0.16.15
  `POST /api` is HTTP 404). Works when the
  Accounts list is still `unavailable` if the host token + loopback URL
  exist. Unauthenticated callers get 401. Password is never logged, never
  returned in JSON, never put on child-process argv. The console sends
  plaintext over the authenticated operator session to loopback Stalwart.
  Stalwart 0.16.15 hashes the secret with
  `Authentication.passwordHashAlgorithm` (default **Argon2id**). See
  [SECURITY.md](SECURITY.md) *Mailbox password hashing*.
  **Contributor self-serve (2026-08-20):** default story is grant npub,
  they sign in with NIP-07, they set their own IMAP password. `PATCH` and
  `POST /api/v1/accounts/password` are both allowed (session-bound CSRF;
  User own mailbox only; Administrator any mailbox as support). User
  `/mail` is "Your mailbox", not the create wall. Evolution User Name is
  the full address (not the local-part alone). Authentication is Normal
  password, not OAuth2. Public mail TLS is **Let's Encrypt** on
  `mail.<apex>:993` / `:465`. The first folder scan can take a long time.
  Evolution may wrap Stalwart `* BYE Connection timed out.` as Failed to
  authenticate; that is a wait, not by itself a bad password.
  **NWC (2026-08-20):** optional Nostr Wallet Connect (NIP-47) save/clear
  at `POST /api/v1/accounts/nwc`. Login stays NIP-07 / NIP-98. NWC is a
  wallet, not login and not IMAP. A NIP-07 **Login failed: 500** on
  `/login` is session exchange, not the NWC store. Store is Domain B
  `/var/lib/surmount/secrets/ui/nwc.json` (`SURMOUNT_NWC_STORE`; Nix
  `nwcStoreFile`). Valid `nostr+walletconnect://` only; nsec and garbage
  refused without echo. Responses never echo the URI. No Lightning node
  in this crate. Alby/NWC extensions are wallets, not the IMAP form.
- **MailPlus extra mailboxes:** living map (who is which uid, which
  aliases, import counts) is `~/.agents/surmount-server/operator-facts.md`.
  Extra people are ordinary **User** mailboxes. **Administrator was not
  granted.** Mailbox passwords were **not** set from SSH (do not invent
  them). Default: grant npub, they log in, they set the password on
  User `/mail`. Boss may still set it on **Set mailbox password**.
  Create more mailboxes on `/mail` Create mailbox before import. Discover
  and mount the NAS that has the Maildir
  (`just diskstation-afp-mount -- --host DS1513` or `--host DS3018xs`).
  Same local-part on another domain is **not** an alias. Distinct MailPlus
  accounts are separate User mailboxes. Extra public MX stays parked.
  Do **not** alias `admin@surmount.systems` onto a person mailbox
  (API-token Admin principal). Do **not** serve unowned domains.
- **Create mailbox + optional npub + two roles shipped (2026-08-14):**
  Administrator `/mail` card **Create mailbox** (local-part, live
  `{local}@{primary_domain}`, optional display name, password, confirm,
  optional npub, role User default or Administrator). One submit creates a
  Stalwart **User** via `x:Account/set`, sets Password the same way (never
  `x:AccountPassword/set`), then writes
  `/var/lib/surmount/console/accounts.json`. Domain id is looked up from
  `SURMOUNT_PRIMARY_DOMAIN` (never a client `domain_id`, never hardcoded).
  Local-part `admin` is reserved. Empty npub is IMAP/SMTP only. User npubs
  stay in the map, not the host allowlist. **Attach npub (2026-08-20):**
  Administrator `/mail` card **Grant console login** pastes bech32 `npub1...`
  (or hex), session-bound CSRF, binds to an existing mailbox so that person
  can log into the services portal (AuthMode nostr). Garbage and nsec are
  refused. Directory listing is **not** required (live default unavailable
  still writes the map). User npubs stay in the map, not the host allowlist.
  Empty npub clears portal login (IMAP/SMTP only). Hunter stays IMAP-only
  until an Administrator pastes a real npub here. Q-AUTH-1 is unchanged.
  Console **User** sees own mailbox password + optional NWC card, no
  create, and 403 on `/system` `/domains` `/accounts`. Administrator
  `/mail` has a two-role story and a mailbox roster (primary, aliases,
  npub yes/no, password yes/no, role). Grant and password use the
  **primary** address (aliases of hunter are not extra people).
  `/accounts` still has no create form.
  `managementUi.directory` stays `unavailable`. MX still parked.
- Remaining: hydrate only where needed; no cargo-leptos dual build until
  islands land; richer queue/metrics if Stalwart exposes them; link to
  runbooks; deeper design-system extract and a11y/mobile polish (see UX
  follow-on report, not this slice).
- Keep Axum as the server integration point (rate-limit, ban, rustls)
- Still not full webmail (that is Phase 3)
- Auth foundation shipped (mode off default); full Q-AUTH-1 residual remains

**Exit criteria:** day-to-day admin no longer requires Stalwart webadmin for
common tasks; `/stalwart-admin/` can be disabled by default.

### Operator console UX principles (living)

1. **Overview = DSM-like package home.** Health strip + large tiles + compact
   alert chips when residual. Not agent residual walls, not env-var dumps as
   the hero, not a second copy of System.
2. **Glanceable tiles first.** One-word or big-number status on tiles (Mail OK/
   Down, Onion Off/Ready/Missing, Vault Off/Linked, domain/account counts).
   Env names and long enable paths belong on **System** (and Login technical
   notes).
3. **Honesty unchanged.** No invent mailboxes, onions, Vaultwarden URLs, or
   host IPs. Empty is fine. Directory unavailable => honest 0 accounts.
4. **Yellow is for true warnings**, not every residual card. Neutral cyan for
   not-provisioned / not-linked setup items.
5. **DOGE only** (pure 3-bit RGB, dark only). No NPM, no second CSS pipeline
   unless operator directs.
6. **System** is the home for technical residual, hostnames table, runbook
   pointers, and JSON API catalog.

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
| GET | `/api/v1/domains` | Config union of primary names (including derived www and mta-sts when first-class), `static_vhosts` keys, and extra mail hostnames. `source` stays `config`. Not Namecheap API. Not Stalwart directory. |
| GET/POST | `/api/v1/accounts` | Directory operations |
| POST | `/api/v1/accounts/console` | Administrator attach/clear npub on a mailbox (session CSRF; map write; listing optional) |
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
