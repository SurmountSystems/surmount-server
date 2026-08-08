# Impl report: research + living-history honesty (26.05)

Date: 2026-08-07
Scope: docs/research rewrite + short living-doc scrub. Option rename was out of scope here and landed in a later pass (`services.stalwart`).
No git commit. No invented package versions (tree + prior labeled audits).

## Living facts verified from tree

| Fact | Source |
|------|--------|
| Host channel `nixos-26.05` | `flake.nix`, `flake.lock` ref |
| Lock rev `445d861c6d31b4af0c79d8d4be2331f762a361d7` | `flake.lock` node `nixpkgs` |
| Crane `rustPackages_1_95` / rustc 1.95 | `nix/rust-toolchain.nix`, `flake.nix` devShell |
| Leptos MSRV floor >= 1.88 | package comments; not living crane pin |
| Dual `disabledModules` | `modules/stalwart-service.nix` (`stalwart-mail.nix` + `stalwart.nix`) |
| Option path is `services.stalwart` (rename landed; unit stays `stalwart-mail.service`) | `modules/stalwart-service.nix`, `modules/mail.nix` |
| Engine still Surmount FOD **0.16.15** (not channel package) | `nix/packages/stalwart-mail.nix`, `COMPACTION-PIN.md` |
| Sample `stateVersion` **26.05** | `hosts/mail-vps/configuration.nix`, tests |

## What changed (bodies, not banners only)

### Primary rewrites

1. **`docs/research/version-audit.md`**
   - Full host/channel/crane rewrite to living 26.05 / `rustPackages_1_95`.
   - Summary + sections 2, 3, 5c, 7, 9, re-run recipe use living facts first.
   - 2026-07-31 / 25.05 / `rustPackages_1_88` only under **historical footnote**.
   - Stalwart FOD / cargo / crates.io "latest" cells still dated 2026-07-30/31
     (not re-polled this pass).
   - Follow-up "plan 25.05 -> 26.05" marked **done**.

2. **`docs/research/stalwart-0.16.15-stores-evidence.md`**
   - Header is living host truth (no "still says 25.05 under a banner").
   - Section 2 rewritten: living 26.05 table + dual disable; past-tense
     footnote for 2026-07-30 host snapshot only.
   - Explicit: do not claim Surmount still boots 25.05.
   - Engine pin remains **0.16.15**; store-shape body kept (still valid for that pin).
   - FOD rationale: host rustc age is historical; vendor 403 still the deferral.

3. **`docs/research/scaffold-assumptions-inventory.md`**
   - Header note for 2026-08-07 host refresh.
   - Section 2: no soft claim that tree is 25.05 / single channel 0.11.8;
     short history + living 26.05 / Surmount FOD.
   - Section 3 dual disable already correct (left).
   - Section 22 already living 26.05 (left).

### Other research present-tense scrub

4. **`docs/research/pqconnect-local-packaging.md`**
   Build evidence labeled historical 25.05 packaging day; re-build on living channel.
5. **`docs/research/pqconnect-and-pqc.md`**
   Version table cells marked "(then)"; living host pointer.
6. **`docs/research/stalwart-stores-evidence-2026-07-30.md`**
   Header no longer "being moved to current"; points at 0.16.15 evidence +
   living 26.05. Historical lock table rows say "then".

### Living docs: short history, no "scaffold accident" romance

7. **`docs/architecture-review.md`** — clean history + FOD rationale (vendor 403;
   not 25.05 rustc as current blocker).
8. **`docs/open-choices.md`** — historical 0.11.8 / 25.05; living 26.05 + FOD.
9. **`docs/DATASTORES.md`** — same clean engine/host line.
10. **`nix/packages/stalwart-mail.nix`** comment — does not claim host channel
    packages 0.11.8 today.

## Intentionally not done (out of scope of this research pass)

- git commit / stage.
- Re-poll GitHub/crates.io for new "latest" (kept prior audit dates).
- Invent stock `pkgs.stalwart` / `pkgs.rocksdb` numbers beyond prior labeled measures.
- Product logic / module behavior changes.

## Follow-up (landed separately)

- Option rename `services.stalwart-mail` -> `services.stalwart` completed in a
  later pass (unit/state remain `stalwart-mail*`). Do **not** re-introduce
  living prose that claims the option path stays `services.stalwart-mail`.
- Soft-debt mop of remaining present-tense 0.11.8 option strings and
  DATASTORES 5.2 TOML eval lies: see `impl-no-debt-mop.md`.

## Residual / honesty notes

- Full upstream re-audit of Stalwart FODs and management-ui Cargo.lock is still
  dated 2026-07-30/31; only host/crane/module path rows were made living.
- Prior pass `.agents/reports/impl-26.05-stale-scrub.md` had left research
  bodies under banners; this pass rewrote those bodies.

## Done criteria

| Criterion | Status |
|-----------|--------|
| version-audit present tense = 26.05 / 1_95 | yes |
| stores-evidence section 2 = living 26.05 + dual disable | yes |
| No research claims Surmount boots 25.05 as current | yes |
| No present-tense rustPackages_1_88 as tree pin | yes |
| Engine still documented as Surmount 0.16.15 FOD | yes |
| Report path | this file |
