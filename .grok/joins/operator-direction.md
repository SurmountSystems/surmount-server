# Join: operator architecture direction docs

**Date:** 2026-07-30
**Scope:** Document operator architecture decisions; no git commit; no clone of Stalwart fork.

## Created

| Path | Role |
|------|------|
| `docs/operator-direction.md` | Dated dump of operator direction 2026-07-30 |
| `docs/principles.md` | Hermetic flakes, versions, system libs/ABI, security, UDS, no nginx |
| `docs/glossary.md` | FOD (Nix), Day-1/Day-2, Fix/FixOS/fixpkgs, DataStore, internal FTS, ... |
| `docs/packages-and-forks.md` | upstream nixpkgs vs Surmount package overlay vs future fixpkgs; Stalwart fork consume; no agent git |

## Updated (living)

| Path | Change summary |
|------|----------------|
| `docs/architecture-review.md` | Supersession banner (host size, RocksDB OK, FTS, edge); table rows + discussion order aligned |
| `docs/STACK.md` | Full rewrite to direction: Rust edge, UDS, RocksDB OK, Nostr, operator VPS |
| `docs/EDGE_AND_TLS.md` | No nginx product; Rust candidates (Axum+tower+rustls-acme, Rama, Pingora); Caddy not preferred; UDS; cutover sketch |
| `docs/SECRETS.md` | sops vs Vaultwarden FAQ; seal still required; LUKS first-class |
| `docs/open-choices.md` | Directed vs open; global Q1-Q9 |
| `docs/hygiene.md` | Aligned rules (edge, host, packages, FTS) |
| `docs/SEARCH_AND_UI.md` | Internal FTS explained; good enough now; own search later |
| `docs/DATASTORES.md` | Direction banner; knobs table for 16G/16c; open Q renumbered |
| `docs/SECURITY.md` | Edge line: Rust target |
| `docs/OPS.md` | nginx transitional-to-delete wording |
| `docs/DNS.md` | operator-chosen VPS PTR; edge wording |
| `docs/fix-and-fixos.md` | L1 packaging landed honesty |
| `README.md` | Doc index + hosting + stack bullets |
| `AGENTS.md` | Direction pins, FOD glossary, fork/git, edge |
| `modules/web.nix` | Header: transitional-to-delete; Rust target |
| `modules/networking.nix` | Comment: UDS / Rust edge |
| `modules/management-ui.nix` | Edge comment |
| `modules/hardening.nix` | Edge-friendly limits comment |
| `secrets/README.md` | Deploy seal vs VW FAQ pointer |

## Not done (by design)

- No Stalwart fork clone
- No nginx removal (marked only)
- No package/code behavior change beyond comments
- No git commit

## Open questions for operator (see operator-direction.md)

- Q1 provider / first-install LUKS
- Q2 fork consume path (flake input vs submodule)
- Q3 keep sops-nix vs alternative deploy seal
- Q4 Rust edge first crate (Axum vs Rama)
- Q5 poolWorkers pin in Nix

## Key pins (one screen)

- Host: operator-chosen VPS, ~16 GB / 2 TB NVMe / 16 cores; not Hetzner-as-default
- RocksDB all-role co-location: fine for now
- Internal FTS: good enough; Surmount search product later
- Edge: no nginx product; prefer Rust + UDS; Caddy not preferred target
- Auth: Nostr keys; Vaultwarden for human/mail cred UX; deploy seal still needed
- Packages: Surmount overlay today; SurmountSystems/stalwart fork when patches; agents never git fork
- blobSize omit; bufferSize omit or 256 MiB optional; poolWorkers omit (=16) or 8
