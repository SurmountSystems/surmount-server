# Fix / FixOS ladder - research notes

**Status:** research finding / working notes. Not operator-accepted product
law. Parent: [../fix-and-fixos.md](../fix-and-fixos.md).

**Last updated:** 2026-07-30

Short practical notes on how Surmount might climb L0-L4 without theater.
No claim that L3/L4 work has started.

---

## Current seed (this repo)

| Signal | Observation |
|--------|-------------|
| Flake input | `nixpkgs` pinned (stable channel style in scaffold) |
| Overlay file | `nix/overlays.nix` present, intentionally light |
| Owned package | management UI via crane under `nix/packages/` |
| Owned modules | `modules/*` Surmount NixOS modules on top of upstream NixOS |
| Hot engine | Stalwart still largely from upstream nixpkgs until a Surmount package lands |

That is **L0 + early L1**, not FixOS.

---

## Practical paths by level

### L0 - consume upstream + overlays

- Pin `flake.lock`. Prefer hashed FODs. No ambient network at eval.
- Use overlays or flake `packages` for one-off pins.
- **Pros:** least ops; stay close to community. **Cons:** channel lag on
  hot packages (mail engines, edge).

### L1 - Surmount-controlled hot packages

- Vendor or `callPackage` current Stalwart (and similar) under
  `nix/packages/` with our features, patches, and version story.
- Keep modules in-repo; override `services.*` package attr.
- **Pros:** solves "stable channel is months behind" without forking NixOS.
- **Cons:** you own rebuilds, CVE follow, and module option drift.

Greenfield rule in [AGENTS.md](../../AGENTS.md): prefer **current majors**;
nixpkgs lag is not a reason to stay old.

### L2 - soft channel (Surmount flake re-export)

- Separate flake (or multi-output monorepo) that exposes:
  - pinned nixpkgs (or a filtered package set)
  - Surmount packages and nixosModules
  - optional `legacyPackages` / overlays for consumers
- Hosts (including `surmount-server`) take that flake as an input.
- **Pros:** one pin set for many hosts; cleaner CI cache boundaries.
- **Cons:** you become a channel operator (docs, bump cadence, breakage
  policy).

Common patterns (community, not Surmount-specific products): flake
composition, `flake-parts` modules for layout, private binary caches
(Attic, Harmonia, S3+signature). Choosing among them is open.

### L3 - FixOS (NixOS lineage)

When L1/L2 still hurt:

- Fork or heavy patch of **module set**, **installer**, **images**, or
  default profiles (security, mail-oriented minimal system).
- Keep consuming upstream nixpkgs packages where possible; own the
  *system* story.
- **Pros:** installer and defaults match Surmount policy. **Cons:** merge
  cost against upstream NixOS; need a clear "what we forked and why" map.

Do not rename every Surmount module "FixOS." FixOS implies a **system
lineage**, not one host flake.

### L4 - Fix (Nix lineage)

Only when evaluator, daemon, store, or CLI policy cannot be met upstream:

- Fork or wrap **Nix** (build/eval/daemon/CLI).
- Highest cost: protocol compatibility, store semantics, team expertise.
- **Pros:** last-resort control. **Cons:** almost always more expensive
  than packaging and modules; climb only with hard evidence.

---

## Adjacency (accurate, not a pick)

Other implementations exist in the broader Nix ecosystem. They are
**adjacency for research**, not Surmount choices unless the operator picks
one.

| Project | Rough role | Note for us |
|---------|------------|-------------|
| **Upstream Nix (CppNix)** | Default evaluator/daemon most flakes use | Scaffold baseline via nixpkgs/NixOS |
| **Lix** | Compatible-ish Nix implementation fork with its own release and community | Possible substitute *evaluator* path; still not "Fix" branding; verify flake/NixOS compatibility before any claim |
| **Snix** | Longer-horizon reimplementation work (store/eval pieces) | Research interest; not a drop-in day-one product path for mail hosts without verification |

Surmount should **not** claim Lix or Snix adoption, dual-stack, or Fix =
Lix. If we ever evaluate alternatives, write a dated evidence note under
`docs/research/` with commands and results.

---

## nixpkgs fork vs soft channel

| Approach | When it helps | Cost |
|----------|---------------|------|
| **Overlays / packages only (L1)** | Few hot packages | Low |
| **Surmount flake re-export (L2)** | Many consumers, shared pins | Medium |
| **Full nixpkgs fork** | Mass package policy, mass patches | High; usually avoid until L2 fails |

Prefer L1 then L2 over a full nixpkgs fork. A full package-set fork is a
different beast from FixOS (modules) or Fix (evaluator).

---

## Climb rules (research summary)

1. Prefer **evidence** (build fail, missing module, policy gap, measured
   lag) over vibes.
2. Greenfield **may** invest early (e.g. current Stalwart package now)
   without pretending L4 is underway.
3. Document the **actual** level in parent [fix-and-fixos.md](../fix-and-fixos.md)
   when status changes.
4. Keep **surmount-server** a host/consumer seed, not the dumping ground
   for entire NixOS history.

---

## Open (mirror of parent)

Repo split, org naming, cache/CI, license/trademark of "Fix", how hard to
push L3, and evaluator adjacency. Answers belong to the operator, not this
note.
