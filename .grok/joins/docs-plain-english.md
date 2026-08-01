# Join: docs plain English (process + open choices)

**Agent:** documentation / process (parallel with Stalwart packaging)
**Date:** 2026-07-30
**Status:** done

## What changed

### Process pins
- Created project [`AGENTS.md`](../../AGENTS.md): no ADR jargon; greenfield prefer current engines; no "unsafe" without evidence; hierarchical docs + subagents; no assumed acceptance.
- Extended global `~/.grok/AGENTS.md`: same no-ADR / greenfield current-versions / evidence-before-unsafe pins (architecture acceptance section already existed; ADR wording demoted).

### Decision docs
- New living file: [`docs/open-choices.md`](../../docs/open-choices.md) (plain English; no ADR-NNN).
- [`docs/DECISIONS.md`](../../docs/DECISIONS.md) is a stub pointer to open-choices.
- Rewrote / scrubbed ADR refs and scare tone in:
  - `docs/STACK.md`, `DATASTORES.md`, `SEARCH_AND_UI.md`, `SECURITY.md`, `SECRETS.md`, `hygiene.md`, `OPS.md`, `EDGE_AND_TLS.md`
  - `README.md`, `secrets/README.md`
  - Module comments: `modules/mail.nix`, `web.nix`, `secrets.nix`, `options.nix` (not package.nix)

### Engine version language
- Stalwart is **being moved to current**.
- **0.11.8** labeled as nixos-25.05 scaffold accident, not product pin.
- Live version: "current (see package after bump)" -> `flake.nix`, `nix/packages/`, `modules/mail.nix` (TODO if packaging adds a dedicated Stalwart package path).

### Research
- [`docs/research/stalwart-stores-evidence-2026-07-30.md`](../../docs/research/stalwart-stores-evidence-2026-07-30.md) header: **historical 0.11.8 evidence only**; re-gather after package bump; section 6 no longer "version skew warning" scare framing.

## Doc layout (intended)

| Path | Role |
|------|------|
| `docs/` | Living top-level docs |
| `docs/open-choices.md` | Undecided / proposed design |
| `docs/research/` | Deep / historical notes |
| `AGENTS.md` | Agent process |

## Not done / left for packaging agent
- Actual Stalwart package version in nix (do not fight `package.nix`).
- After bump: new research evidence file; fix any TODO package links.
- Did not rewrite every historical "0.11.8" fact line inside the research body (still correct as history).

## Grep check (product docs)
- Remaining "ADR" mentions are bans/explanations in `AGENTS.md`, `hygiene.md`, and the DECISIONS stub, not product ADR-NNN IDs.
