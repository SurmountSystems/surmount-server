# Stalwart 0.16.x directory / accounts listing (research)

**Date:** 2026-08-01 (live list client + mutations shipped 2026-08-07)
**Status:** research finding + tree status (not operator-accepted product law)
**Purpose:** How Surmount lists mail principals/accounts without inventing fake
`admin@` rows in the management console.

Related: [SEARCH_AND_UI.md](../SEARCH_AND_UI.md), [MIGRATION.md](../MIGRATION.md),
[STACK.md](../STACK.md), residual Admin Leptos SSR track.

## Summary

Stalwart 0.16 manages accounts as **management objects** over the same JMAP
management API used by `stalwart-cli` and the webadmin. Surmount's
`GET /api/v1/accounts` defaults to **honest empty** (`source: unavailable`)
until explicit opt-in: `SURMOUNT_DIRECTORY=stalwart`, host token, and auth
coupling (see **Shipped**). The live client is shipped; default stays empty.
Do **not** ship fake inventory.

## Upstream CLI path (primary operator tool today)

`stalwart-cli` (pinned in-tree; see COMPACTION-PIN package table) speaks the
management JMAP API. Docs:

- Overview: https://stalw.art/docs/management/cli/
- List/filter: https://stalw.art/docs/management/cli/query
- Schema explore: https://stalw.art/docs/management/cli/describe

Examples for operators (auth and URL match your deploy; loopback common):

```bash
export STALWART_URL="http://127.0.0.1:8080"
# STALWART_USER / STALWART_PASSWORD or CLI flags per version

stalwart-cli describe              # list management object types
stalwart-cli query account         # list accounts (default columns)
stalwart-cli query account --fields id,name,domainId --json
stalwart-cli query domain
```

`query` is paginated and can emit NDJSON with `--json`. Filters use
`--where key=value`. Object names are case-insensitive and may omit the `x:`
prefix used on the wire.

## Relation to principals / directory

Stalwart still uses **principal** concepts for authz (individual, group, etc.)
and may expose LDAP or other directory backends as configuration objects. For
day-to-day mailbox listing on a single-host internal directory, the practical
SoT for operators is:

1. **stalwart-cli** `query account` / create-update flows, or
2. **Webadmin** (bootstrap; Surmount treats this as fallback only), or
3. Future: Surmount management-ui authenticated proxy to the same management
   API (residual; needs Nostr or other product auth first).

Internal FTS / message store is **not** the account directory. Do not scrape
RocksDB for mailboxes.

## Surmount tree today

| Surface | Behavior |
|---------|----------|
| `GET /api/v1/accounts` | Via `AppState.directory`; default empty + `source: "unavailable"` |
| SSR `/accounts` | Same directory strategy; next-step guidance to cli / bootstrap admin |
| Domains | Config inventory (`SURMOUNT_*` hostnames), not engine directory |
| JMAP proxy | `POST /api/v1/jmap` → 501 (mail JMAP proxy; distinct from management `/api`) |
| Directory strategy | `crates/management-ui/src/directory.rs`: trait + `UnavailableDirectory` (default) + hermetic `MockDirectory` (`source: "mock"`) + live `StalwartDirectory` (`source: "stalwart"`, explicit only) |

**Shipped (2026-08-07):** directory trait + hermetic mock + **live** Stalwart
management JMAP client (`x:Account/query` + `x:Account/get` against
`SURMOUNT_STALWART_URL/api` with Bearer token). Selection:

| `SURMOUNT_DIRECTORY` | Backend |
|----------------------|---------|
| unset / empty / `unavailable` | Honest empty (default; never invents rows) |
| `mock` | Hermetic fixture only (tests / lab; never production default) |
| `stalwart` | Live client; requires `SURMOUNT_STALWART_TOKEN` or `SURMOUNT_STALWART_TOKEN_FILE` (host-only; fail-closed at process start if missing) |
| other | Fail-closed at process start (typo not silent skip) |

Live list failures (HTTP error, unreachable, unparseable): empty accounts +
error note, **source stays `"stalwart"`** (no invented rows). Hermetic wire-mock
tests cover happy path + fail-closed paths; live Stalwart not required for CI.

Nix: `surmount.managementUi.directory` (default `unavailable`),
`stalwartTokenPath` / `stalwartTokenEnv` (lab). Token file on ReadOnlyPaths +
`ConditionPathExists` when path-only so missing host secret stays inactive
(not restart thrash).

**Auth coupling (2026-08-07 fix pass):** live backend requires
`SURMOUNT_AUTH_MODE=nostr` (or Nix `authMode=nostr`) unless lab
`SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1` /
`allowDirectoryUnauthenticated`. Token file: non-empty raw line, regular file,
owner-only mode (e.g. 0600). HTTP error notes are status-code only (no body).

**Mutations shipped (2026-08-07):** `Directory::create_account` /
`update_account`; HTTP `POST /api/v1/accounts` and `PATCH /api/v1/accounts/{id}`;
mock fixture mutates in-memory; live uses management JMAP `x:Account/set`
(create/update; lean fields: name + domainId + optional description; no
password/credentials in this surface). Gate: `AUTH_MODE=nostr` or lab
`SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1`; CSRF double-submit when session
cookie present. No HTML create form (API only). Hermetic wire-mock covers
happy path + fail-closed HTTP.

**Still residual:** full Q-AUTH-1 product answers; UDS to Stalwart; mail
password / credential rotation product UX (stay stalwart-cli for secrets).

## Migration note

[MIGRATION.md](../MIGRATION.md) already requires creating the Stalwart account
before Maildir import (`stalwart-cli` or admin UI). Directory wire-up does not
replace that bootstrap step.

## Non-claims

- **In-tree:** live directory **list** client is implemented (explicit opt-in;
  hermetic wire-mock covered). This note is research + tree status, not a
  second implementation.
- **Not claimed:** host cutover, live Tor ownership, live ban drop, or that
  default production exposes live inventory (default remains unavailable).
- **In-tree mutations:** create/update API is implemented behind auth + CSRF
  (no HTML form; no password fields). Host live mutation proof still residual.
- **Do not invent Q-AUTH-1** (key-loss, durable session store, first-operator
  bootstrap). Live directory and mutations require `authMode=nostr` unless lab
  escape (`allowDirectoryUnauthenticated` / `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED`).
- Port 8080 vs UI 8090: module defaults use Stalwart HTTP on loopback 8080 and
  UI on 8090; confirm on your host.
