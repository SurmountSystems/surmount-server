# Join: architecture + datastore review

**Date:** 2026-07-30
**Status:** docs synthesis complete. No package/module edits. No commit.
**Workspace:** `/home/hunter/Projects/surmount/surmount-server`

## Deliverable

- **Main writeup:** [`docs/architecture-review.md`](../../docs/architecture-review.md)
- **Cross-links:** `docs/open-choices.md` (top pointer), `README.md` Architecture section

## Inputs read fully

- `docs/research/stalwart-0.16.15-stores-evidence.md`
- `docs/research/scaffold-assumptions-inventory.md`
- `docs/fix-and-fixos.md`
- `docs/open-choices.md`
- `.grok/joins/stalwart-current.md`
- `.grok/joins/stalwart-016-evidence.md`
- `.grok/joins/scaffold-assumptions.md`
- Also skimmed for consistency: `docs/DATASTORES.md`, `docs/STACK.md`, `README.md`, module port greps

## What the review contains

1. How to read fact vs scaffold vs open; Fix/FixOS ladder as **stated agreement on direction only**
2. Version/source table (0.16.15 / cli 1.0.12 / webui 1.0.7 / spam 3.0.0 / nixos-25.05 lock)
3. Mail engine on 0.16.15: config.json DataStore-only, four roles, what Surmount wires
4. Datastore deep dive per role + physical backends + unproven assumptions + operator questions
5. Full stack assumption table (identity, UI, edge, secrets, topology, Fix, migration, ops)
6. Recommended discussion order (engine -> packaging -> data plane -> listeners -> edge -> UX -> secrets -> migration -> Fix height -> ops)
7. Stop-implying list (doc lag STACK/MIGRATION/DATASTORES/README ports, plus false "decided" claims)

## Tone / constraints

Plain American English. No ADR jargon. No "you accepted X" except Fix/FixOS ladder direction per task. ASCII. Absolute paths not required in doc (repo-relative links). Senior peer-review voice with versions and sources.

## Not done

- No living-doc full port sync (STACK/MIGRATION still lag; listed as cleanup candidates)
- No module or package changes
- No git commit
