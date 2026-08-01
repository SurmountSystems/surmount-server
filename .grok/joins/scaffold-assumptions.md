# Join: scaffold assumptions inventory

**Date:** 2026-07-30
**Status:** docs-only research done. Not operator acceptance.
**Workspace:** `/home/hunter/Projects/surmount/surmount-server`

## Deliverable

- **Inventory:** [`docs/research/scaffold-assumptions-inventory.md`](../../docs/research/scaffold-assumptions-inventory.md)
- **27 sections** covering every requested topic plus material extras (listeners, domains, ACME/TLS gap, hermetic flake, hosting, self-ops, doc lag).

## What was read

Living docs: `open-choices`, `STACK`, `DATASTORES`, `fix-and-fixos`, `SEARCH_AND_UI`, `SECURITY`, `SECRETS`, `EDGE_AND_TLS`, `OPS`, `MIGRATION` (head), `hygiene` (head), `README`.
Modules: all `modules/*.nix` (headers + key options/ports).
UI stack: `crates/management-ui/Cargo.toml` (Axum only; no Leptos yet).
Joins: `stalwart-current.md`, `foundation.md`, `fix-fixos.md`.
Packages: `nix/packages/*` listing; host sample defaults.

## Method

Each assumption row: claim, paths, why it might be there, status
(scaffold / proposed / required by code / unknown), revisit for Stalwart
**0.16.15**, one operator question. Plain English. No ADR. No acceptance claims.

## Highest-signal findings (short)

1. **Code ahead of some docs:** packages + modules are Stalwart **0.16.15** binary FOD, config.json RocksDB, UI **:8090**, Stalwart HTTP **:8080**. STACK/MIGRATION/foundation still mention :8081 / old UI :8080 / TOML in places.
2. **Required by code today:** Surmount `stalwart-service.nix` (nixpkgs TOML module disabled), RocksDB path, nginx edge, firewall ports, management-ui Axum skeleton, FOD install under `/etc/surmount/stalwart/`.
3. **Proposed only:** Caddy target, Nostr auth, Leptos SSR, Vaultwarden, LUKS, multi-host forever-not, external FTS, Fix L2+.
4. **Known 0.16 gaps:** spam/webui FODs not auto-applied; first-boot listeners may bind :8080/:443 broadly; import helper CLI may need confirm; Stalwart mail TLS cert wiring TODO.
5. **Store co-location:** scaffold default with gates in DATASTORES; not operator-accepted.

## Not done (out of scope)

- No package or module edits.
- No living-doc port/version sync (listed as section 27 + operator question).
- No git commit.

## Operator next

Optional: walk inventory sections 1-3, 13, 19, 4, 15 before install; answer open questions into `docs/open-choices.md` when ready.
