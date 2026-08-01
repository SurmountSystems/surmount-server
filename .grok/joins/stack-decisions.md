# Join: stack decisions documentation (2026-07-30)

## Goal

Pin operator architecture decisions to durable docs: Stalwart storage,
search/UI split, Leptos + minimal-web/desktop, no Cloudflare, edge without
nginx as target, self-ops, hygiene consistency. No git commit. No nginx
cutover rewrite.

## Research facts (verified)

### Stalwart storage

- Official model: four stores (data, blob, search/FTS, in-memory).
  Docs: https://stalw.art/docs/install/store/ , https://stalw.art/docs/storage/ ,
  https://stalw.art/docs/storage/fts/ , https://stalw.art/docs/storage/backends/rocksdb/
- Single-node common pattern: RocksDB for all four.
- nixpkgs 25.05 `services.stalwart-mail` with `stateVersion >= 24.11`
  (host uses **25.05**): `store.db.type = rocksdb`, path
  `/var/lib/stalwart-mail/db`, compression lz4; `storage.{data,fts,lookup,blob}`
  all default to `"db"`.
- Flake eval confirmed RocksDB store on this tree.
- Legacy path only for older stateVersion: SQLite + filesystem blobs.
- FTS default: internal (data-store-backed / bloom-filter style). ES/Meilisearch
  optional later, not default.

### Edge recommendation

- Compared Caddy, Traefik, HAProxy+ACME, pure Rust edge, Envoy, stay-nginx.
- **Target default: Caddy** (ACME built-in, simple reverse proxy, nixpkgs
  module maturity, fits single VPS). nginx remains **transitional** in tree.
- Pure Rust edge deferred as higher ownership cost.
- No CF required path.

## Files written / updated

| Path | Action |
|------|--------|
| `docs/STACK.md` | **Created** - full stack map |
| `docs/DECISIONS.md` | **Created** - ADR-001..011, date 2026-07-30 |
| `docs/SEARCH_AND_UI.md` | **Created** - phases 0-5, search split |
| `docs/EDGE_AND_TLS.md` | **Created** - candidates, Caddy target, no CF |
| `docs/OPS.md` | **Created** - logs, health, scripts, runbooks |
| `docs/hygiene.md` | **Updated** - no CF, edge policy, doc survival, Leptos, desktop, RocksDB/search |
| `README.md` | **Updated** - Architecture section + links; Leptos; nginx transitional; scripts |
| `modules/web.nix` | **Light** - transitional header comment pointing at EDGE_AND_TLS |
| `modules/networking.nix` | **Light** - edge wording |
| `flake.nix` | **Light** - description drop hard-coded nginx identity |
| `scripts/check-dns.sh` | **Created** |
| `scripts/check-tls.sh` | **Created** |
| `scripts/check-mail-ports.sh` | **Created** |
| `scripts/README.md` | **Created** |
| `.grok/joins/stack-decisions.md` | this join |

## Explicit non-actions

- No git commit.
- No nginx -> Caddy implementation cutover (docs + TODO comments only).
- No product code rewrite of management-ui to Leptos this turn.
- No change to Stalwart store settings (already RocksDB via nixpkgs defaults).

## ASCII / hygiene

- New/updated prose uses ASCII dashes (no em dash).
- No secrets added.
- Scripts are read-only smoke helpers; bash -n clean.

## Implement follow-ups (for later agents)

1. Optional `surmount.web.backend = "nginx" | "caddy"` and Caddy cutover per EDGE_AND_TLS.
2. Wire Stalwart TLS cert paths from ACME once edge certs exist.
3. SEARCH_AND_UI Phase 1+ (auth, real directory APIs, JMAP proxy, Leptos).
4. Promote scripts to flake apps if useful.
5. Host-level journald retention once disk size known.

## Acceptance

Another agent can implement stack work from `docs/STACK.md` +
`docs/DECISIONS.md` without re-asking product questions pinned in the
operator mission.
