# Join: Fix / FixOS naming and ownership docs (2026-07-30)

## Mission

Document operator **working direction** for Surmount-owned Nix/NixOS lineage
names (**Fix**, **FixOS**, **Facta Non Verba**). Open direction only; not
operator-accepted product law. Additive alongside Stalwart packaging and
plain-English doc work. No git commit. No reverse of packaging work.

## Done

### New parent doc

- `/home/hunter/Projects/surmount/surmount-server/docs/fix-and-fixos.md`
  - Names + etymology (Facta Non Verba)
  - Why (current engines, ownership, defaults)
  - Honest fork ladder L0-L4
  - This repo = early FixOS **consumer/seed**, not full Fix monorepo
  - Naming rules (Fix/FixOS vs upstream Nix/NixOS/nixpkgs)
  - Open questions for operator
  - Status language: proposed / research / open only

### New research child

- `/home/hunter/Projects/surmount/surmount-server/docs/research/fix-fixos-ladder.md`
  - Practical L0-L4 paths
  - Current seed = L0 + early L1
  - Soft channel vs full nixpkgs fork
  - Lix/Snix adjacency as research only (no adoption claim)
  - Climb rules

### Light links / process pin

- `docs/open-choices.md` - living maps + short Fix/FixOS section
- `README.md` - one table row to `docs/fix-and-fixos.md`
- `AGENTS.md` - Fix/FixOS naming bullet under Language
- **Skipped** `docs/hygiene.md` for contested mid-rewrite risk (still carries
  older ADR phrasing); parent doc links hygiene; pin lives in AGENTS.md

## Honest level today

- **Not claimed:** L2 soft channel, L3 FixOS fork, L4 Fix evaluator work.
- **Claimed in docs:** L0 consume + early L1 (management-ui package, Surmount
  modules); Stalwart current packaging is other agents' track.

## Rules honored

- ASCII only, no em dashes
- No "ADR" in new Fix/FixOS docs
- No "operator accepted"
- No git commit
- Hierarchical parent + research child
- Additive; did not touch Stalwart package sources

## Key paths

- `docs/fix-and-fixos.md`
- `docs/research/fix-fixos-ladder.md`
- `AGENTS.md` (naming pin)
- `docs/open-choices.md`, `README.md` (light links)
