# Fix and FixOS (working direction)

**Status:** open working direction / research naming. Not operator-accepted
product law unless you say so in writing. Prefer **proposed**, **scaffold
default**, or **research finding** until then.

**Last updated:** 2026-07-30

Child research (ladder detail): [research/fix-fixos-ladder.md](research/fix-fixos-ladder.md).
Living maps: [STACK.md](STACK.md), [open-choices.md](open-choices.md),
[hygiene.md](hygiene.md). Process pins: [AGENTS.md](../AGENTS.md).

---

## 1. What the names mean

| Name | Meaning |
|------|---------|
| **Fix** | Surmount's take on, or fork lineage of, **upstream Nix**: package manager, language, evaluator, daemon/CLI story. |
| **FixOS** | Surmount's take on, or fork lineage of, **upstream NixOS**: system modules, images, installer, module-system defaults. |
| **Facta Non Verba** | Latin: "deeds, not words." Cultural root of the Fix / FixOS names. |

**Fix is intentional branding, not a typo for "Nix."** The rhyme is
deliberate: own the stack with action, not theater.

### Facta Non Verba (short)

Surmount prefers **build and ship** over design ceremony. Docs describe what
we build and what is still open. They do not pretend words replace working
hosts, packages, and modules. Fix / FixOS are names for owned engineering
lineage if and when we need it; they are not a slogan campaign.

---

## 2. Why this might exist

Greenfield Surmount wants **current engines** (for example current Stalwart),
not a forever pin to whatever a stable channel last packaged. Channel lag is
a measured fact of how nixpkgs ships, not a moral failure of upstream, and
not a reason for us to ship stale mail software.

Owning packaging, modules, and policy lets Surmount:

- Ship **current majors** without waiting on slow stable channels or
  upstream review bandwidth.
- Set **Surmount-specific defaults** (security posture, hermetic flake
  discipline, mail/web stack shape) without arguing every knob into
  community nixpkgs.
- Keep a clear story for operators: what is Surmount-owned vs what is
  community upstream.

None of that requires day-one monorepo forks of Nix and NixOS. It requires
an honest ladder (next section) and evidence when we climb.

---

## 3. Layers of "fork" (honest ladder)

Do **not** pretend a full monorepo fork is required on day one. Climb when
**evidence** shows need (failed package, missing module, policy we cannot
express cleanly), or when greenfield deliberately invests early. Still do
not claim a higher layer is started if the tree only has lower-layer work.

| Level | Name (informal) | What it is | Typical trigger |
|-------|-----------------|------------|-----------------|
| **L0** | Consume + overlay | Upstream nixpkgs + overlays / flake packages in this repo | Scaffold start; most day-to-day |
| **L1** | Owned hot packages | Surmount-controlled packages and modules for critical engines (current Stalwart, management UI, etc.) while still using upstream Nix and NixOS | Channel lag, broken or empty package path, need our build flags |
| **L2** | Soft channel | Surmount flake/channel that re-exports a pinned nixpkgs plus our packages | Multiple hosts/repos need the same pin set; CI cache story |
| **L3** | **FixOS** | Forked or heavily patched NixOS module set, installer, images, or defaults | Module system or installer cannot express our policy without constant re-patch |
| **L4** | **Fix** | Forked or wrapped Nix evaluator / daemon / CLI | Upstream Nix cannot serve us (eval, daemon, store policy, tooling) after L0-L3 exhausted |

**Today in `surmount-server` (honest):** **L0** plus **L1 in progress**:
Surmount package overlay owns Stalwart FODs (`nix/packages/stalwart-*.nix`),
custom 0.16 service module, crane-built management UI, `nix/overlays.nix`.
Host OS still comes from upstream nixpkgs. Plain names: upstream nixpkgs +
Surmount package overlay; future fixpkgs channel not started.
**L2-L4 are not started** as separate Fix / FixOS products. See also
[packages-and-forks.md](packages-and-forks.md).

Deeper practical notes: [research/fix-fixos-ladder.md](research/fix-fixos-ladder.md).

---

## 4. Relationship to this repo (`surmount-server`)

This repository is an early **FixOS consumer** and **seed** for Surmount
mail + web hosts. It is **not** the whole Fix monorepo, and it is not
claiming to be FixOS itself.

What lives here today:

- Flake-locked **upstream nixpkgs** (and related inputs).
- Surmount **NixOS modules** (`modules/`) for mail, web, secrets, hardening.
- Surmount **packages** (`nix/packages/`, crane) where we own the build.
- Product Rust (`crates/`), ops docs, and host sample (`hosts/mail-vps/`).

What does **not** live here yet (open; see questions below):

- A separate Fix or FixOS git org or monorepo.
- A public Surmount soft channel (L2).
- Forked NixOS modules tree or installer branded FixOS (L3).
- Forked Nix evaluator/daemon branded Fix (L4).

If FixOS becomes real, this repo should stay a **consumer** (and maybe a
reference host set), not absorb all of NixOS history into the mail stack.

---

## 5. Naming rules

| When you mean... | Prefer saying... |
|------------------|------------------|
| Surmount-owned Nix lineage (evaluator/CLI/store story) | **Fix** |
| Surmount-owned NixOS lineage (modules/images/defaults) | **FixOS** |
| Community package manager / language / Nix project | **upstream Nix** |
| Community NixOS / module system | **upstream NixOS** |
| Community package set / channel pin | **nixpkgs** (optionally "upstream nixpkgs") |
| This repo's modules and packages only | **surmount-server**, Surmount modules, Surmount packages |

Do not call every overlay "FixOS." Do not call nixpkgs "Fix." Keep the
ladder honest so docs stay readable after compaction.

---

## 6. Open questions (for the operator)

Listed as questions, not answers.

1. **Repo layout:** keep packaging in `surmount-server`, or split
   `fixpkgs` / `fixos` / host flakes early?
2. **Git orgs and names:** SurmountSystems under one org, or separate
   public Fix / FixOS identities?
3. **CI and binary cache:** Hydra, Attic, public cache, private cache only?
4. **How hard to fork:** L1-L2 only for years, or invest toward L3 FixOS
   images while greenfield is cheap?
5. **License:** stay aligned with upstream Nix/NixOS licenses on forks;
   any Surmount policy on dual-license or trademark of "Fix"?
6. **Binary cache strategy vs purity:** who signs, what is substitutable,
   offline rebuild story for mail hosts?
7. **Relationship to other Nix evaluators:** any deliberate adjacency to
   Lix, Snix, or other implementations, or stay on upstream CppNix until
   evidence forces a choice? (See research note; no pick claimed here.)
8. **Channel product:** is a public "soft channel" (L2) a product Surmount
   offers, or only an internal pin set?

---

## 7. Facta Non Verba again

Docs name the direction. **Deeds** are packages that build, modules that
boot, hosts that send mail, and evidence notes when we climb a ladder rung.
Words without that are not Fix.

---

## See also

- [research/fix-fixos-ladder.md](research/fix-fixos-ladder.md) - practical paths per level
- [open-choices.md](open-choices.md) - other undecided design
- [STACK.md](STACK.md) - host stack map
- [hygiene.md](hygiene.md) - engineering rules for this repo
- [AGENTS.md](../AGENTS.md) - agent process and naming pins
