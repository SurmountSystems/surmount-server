# Plan 26.05 stale scrub inventory

Date: 2026-08-07
Scope: remaining incorrect **current** pins for host channel `nixos-25.05`, crane `rustPackages_1_88` / living rustc 1.88, and dual `disabledModules` / rename documentation gaps.
Method: tree-wide `rg` plus targeted reads of code and living docs. No product edits.

## Summary

Product **code** already tracks **nixos-26.05**, crane **`rustPackages_1_95`**, sample-host / test **`system.stateVersion = "26.05"`**, and **dual** stock-module disable (`stalwart-mail.nix` + `stalwart.nix`). Living pins in COMPACTION-PIN, packages-and-forks (channel row), rust-toolchain, flake, hosts, and most of RESIDUAL match that.

What remains is mostly **doc and comment scrub**: a few living product rows still quote 25.05 or singular "TOML module," one **wrong flake.lock rev** in architecture-review, management-ui Cargo.toml still says crane uses `rust_1_88`, and research files still speak in **present tense** as if Surmount boots 25.05 / builds with `rustPackages_1_88`. Dual disable is **implemented**; living docs under-sell it in places and one DATASTORES unknown still treats the 26.05 rename as open.

## Already fixed (do not re-plan as open work)

These already match 26.05 / 1.95 / dual disable (or correctly frame MSRV vs toolchain):

| Area | Status |
|------|--------|
| `flake.nix` | `nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05"`; comments; crane via `rust-toolchain.nix`; devShell `rustPackages_1_95` |
| `flake.lock` | `nodes.nixpkgs.original.ref = "nixos-26.05"`; rev `445d861c6d31b4af0c79d8d4be2331f762a361d7` (not the old `ac62194c...` 25.05 tip) |
| `nix/rust-toolchain.nix` | `rustPackages_1_95`; name `surmount-rust-1.95`; notes 1.88 set removed on 26.05 |
| `nix/packages/management-ui.nix` | Comment: nixos-26.05 + `rustPackages_1_95`; MSRV floor 1.88+ only |
| `crates/Cargo.toml` (workspace) | Comment: MSRV 1.88+; crane uses `rustPackages_1_95` on nixos-26.05 |
| `hosts/mail-vps/configuration.nix` | `system.stateVersion = "26.05"` |
| `tests/module-eval.nix`, `tests/mail.nix` | `stateVersion = "26.05"` |
| `modules/stalwart-service.nix` | Dual `disabledModules`: both stock paths; option path stays `services.stalwart-mail` |
| `docs/COMPACTION-PIN.md` | Crane 1.95 / `rustPackages_1_95`; host nixos-26.05 |
| `docs/packages-and-forks.md` | Upstream nixpkgs row: **nixos-26.05** |
| `docs/hygiene.md` | 0.11.8 framed as pre-26.05 historical accident |
| `docs/architecture-review.md` | Host OS channel row **nixos-26.05**; management-ui crane `rustPackages_1_95`; hermetic flake claim "host on 26.05" |
| `docs/DATASTORES.md` | Section 5.1 dual paths + stateVersion 26.05; Surmount not on legacy store path |
| `RESIDUAL.md` (Leptos crane bullet) | `rustPackages_1_95` + host nixos-26.05 |
| `docs/research/scaffold-assumptions-inventory.md` section 22 | Living claim: hermetic flake, 26.05 host, crane wants >= 26.05 |
| `open-choices.md` | 0.11.8 as **historical** nixos-25.05 scaffold accident only |
| `AGENTS.md`, `README.md`, `STACK.md` | No living 25.05 / `rustPackages_1_88` host pin |

## Living docs still stale (file:line + quote + recommended fix)

### High priority (wrong as current fact)

1. **`docs/architecture-review.md:89`**
   Quote: ``| nixpkgs lock rev | `ac62194c3917d5f474c1a844b6fd6da2db95077d` | locked node `nixpkgs` | `flake.lock` |``
   **Issue:** That rev was the old **nixos-25.05** tip. Current lock is **26.05** at `445d861c6d31b4af0c79d8d4be2331f762a361d7`.
   **Fix:** Re-read `flake.lock` and update the table (or drop the rev from living pin table and point at lock only).

2. **`crates/management-ui/Cargo.toml:34`**
   Quote: `# SSR-only admin shell (no hydrate / WASM / NPM). MSRV 1.88; crane uses rust_1_88.`
   **Issue:** Crane pin is **`rustPackages_1_95`**, not `rust_1_88`.
   **Fix:** Align with workspace comment: MSRV 1.88+; crane uses `rustPackages_1_95` on nixos-26.05. Keep line 45 `crane rustc is 1.88+` only if it means MSRV floor (optional clarify).

3. **`docs/DATASTORES.md:463-464`**
   Quote: `Matches nixpkgs 25.05 non-legacy defaults (we independently agree for now; we do not outsource judgment).`
   **Issue:** Host channel is 26.05; rationale should not sound like we still match a 25.05 pin.
   **Fix:** Rephrase to historical "matched stock non-legacy RocksDB defaults (including on older 25.05 modules)" or "matches stock non-legacy RocksDB defaults" without naming 25.05 as current.

4. **`docs/DATASTORES.md:644-645`**
   Quote: `Whether nixpkgs will rename services.stalwart-mail -> services.stalwart on 26.05+ (watch release notes).`
   **Issue:** Module comments and section 5.1 already treat **26.05+** as having `services/mail/stalwart.nix` and rename to `services.stalwart`. "Whether" is outdated as an open unknown for stock nixpkgs on this channel.
   **Fix:** State rename as known on 26.05 stock modules; Surmount keeps `services.stalwart-mail` and dual-disables both paths. Residual (if any) is tracking upstream module shape, not "if rename happens."

### Medium priority (singular disable / imprecise living wording)

5. **`docs/architecture-review.md:91`**
   Quote: `disables nixpkgs TOML module` (singular).
   **Fix:** Dual disable: `services/mail/stalwart-mail.nix` (older) and `services/mail/stalwart.nix` (26.05 rename path); Surmount keeps `services.stalwart-mail`.

6. **`docs/architecture-review.md:176`**
   Quote: `nixpkgs module | **disabled**; Surmount owns modules/stalwart-service.nix`
   **Fix:** Same dual-path wording as above.

7. **`docs/architecture-review.md:358`**
   Quote: `nixpkgs TOML module off` / `` `disabledModules` `` without both paths.
   **Fix:** Name both module files.

8. **`docs/packages-and-forks.md:49`**
   Quote: `0.16+ config.json service (disables nixpkgs TOML module)`
   **Fix:** Disables **both** stock module paths; option path stays `services.stalwart-mail`.

9. **`docs/architecture-review.md:90`**
   Quote: `Channel pkgs.stalwart-mail | still **0.11.8** on 25.05 | historical only`
   **Issue:** Host is 26.05; "still ... on 25.05" confuses current channel package with history. Stock package version on **26.05** may differ (research noted 0.15.x on 26.05).
   **Fix:** Either eval current channel `pkgs.stalwart-mail` version for 26.05, or label clearly as "historical on 25.05; Surmount never uses channel engine."

10. **`docs/architecture-review.md:102`**
    Quote: `CLI wants rustc newer than 25.05's 1.86`
    **Issue:** Acceptable as historical packaging rationale; lightly misleading next to a 26.05 host table.
    **Fix:** "older host channels' default rustc (e.g. 25.05's 1.86)" or drop version numbers.

11. **`RESIDUAL.md:391`**
    Quote: `crane rustc 1.88 vs stable 1.97.1 noted` (checked agent-done item).
    **Issue:** Archive of pre-bump audit; not a living pin, but readers may think crane is still 1.88.
    **Fix:** Parenthetical "pre-26.05 audit; living crane is 1.95 via rustPackages_1_95" or point only at COMPACTION-PIN.

### Acceptable historical living phrasing (no mandatory scrub)

- `docs/DATASTORES.md:22`, `docs/open-choices.md:84`, `docs/hygiene.md:67`: 0.11.8 / 25.05 as **scaffold accident** history.
- `docs/COMPACTION-PIN.md:86`: Leptos **MSRV >= 1.88** with living crane **1.95**. Correct distinction.
- `modules/stalwart-service.nix:4-5,45`: "nixos-25.x path + 26.05 path" comments document dual disable. Correct.

### Code: no pin/stateVersion bump left

- No remaining product `.nix` using `stateVersion = "25.05"`.
- No remaining product code referencing `rustPackages_1_88`.
- Dual `disabledModules` already present in `modules/stalwart-service.nix`.

## Research present-tense lies (only if any)

Leave dated research alone when framed as a past audit **except** present-tense claims that imply the **tree today** still boots 25.05 or pins `rustPackages_1_88`.

### Clear present-tense / tree-as-today lies

1. **`docs/research/stalwart-0.16.15-stores-evidence.md:112-126`**
   - "Surmount still boots the host from **nixos-25.05**."
   - Table: flake input `nixos-25.05`, rev `ac62194c...`, `25.05pre-git`.
   - Disables only `stalwart-mail.nix` / "25.05 TOML module".
   **Fix if touched:** Banner "evidence as of YYYY-MM-DD; host channel later moved to nixos-26.05; dual disabledModules since ..." or rewrite section 2 to past tense. Do not treat as living SoT.

2. **`docs/research/version-audit.md`** (summary and sections still present-tense gap language)
   - L41: crane **1.88.0** (`rustPackages_1_88`).
   - L43, L49: pin is `nixos-25.05`; OS channel operator-gated on 25.05.
   - L97, L112-113, L250-251, L281, L324, L344: full 25.05 / 1.88 audit body.
   **Fix if touched:** Top banner "snapshot 2026-07-31; superseded for host channel and crane by 26.05 / rustPackages_1_95 (see COMPACTION-PIN)." Do not re-run as living audit without a new pass.

3. **`docs/research/scaffold-assumptions-inventory.md:91-101`** (section 3)
   - Still claims only `services/mail/stalwart-mail.nix` is disabled; "25.05 and unstable modules still assume older config."
   - Section 22 already updated for 26.05.
   **Fix if touched:** Dual disable + `stalwart.nix` rename path; past-tense 25.05 where historical.

### Research that is historical framing (leave alone unless scrub wave)

- `docs/research/stalwart-stores-evidence-2026-07-30.md` (dated filename; 25.05 as then-fact).
- `docs/research/pqconnect-*.md` (builds against 25.05 as experiment).
- Historical 0.11.8 / 25.05 package lag tables used as contrast.

## Dual disabledModules / rename: code status + doc gaps

### Code status (done)

```nix
# modules/stalwart-service.nix
disabledModules = [
  "services/mail/stalwart-mail.nix"  # older / 25.x TOML path
  "services/mail/stalwart.nix"       # 26.05+ rename path
];
# options.services.stalwart-mail kept; unit name stalwart-mail.service
```

Comments already explain: 26.05+ renames stock options to `services.stalwart` and would hijack Surmount options without dual disable. No further module code change required for this inventory.

### Doc gaps

| Doc | Gap |
|-----|-----|
| architecture-review (pins + section 2.4 + challenge table) | Singular "TOML module"; no `stalwart.nix` |
| packages-and-forks | Singular disable in module table |
| DATASTORES section 5.1 | **Already good** on dual paths and rename |
| DATASTORES section 10 unknowns | Still open-questions the rename |
| COMPACTION-PIN | Does not mention dual disable (optional pin row; not wrong) |
| scaffold-assumptions section 3 | Single path only; section 22 channel OK |
| research 0.16.15 stores evidence | Only old path + 25.05 host |

## Proposed plan slices (ordered, small, agent-doable)

1. **Living pin table accuracy**
   Update `docs/architecture-review.md` nixpkgs lock rev from `flake.lock`; fix channel `pkgs.stalwart-mail` row for 26.05 or pure history; dual-disable wording in rows 91, 176, 358.

2. **management-ui Cargo.toml comment**
   One-line fix: crane `rustPackages_1_95`, not `rust_1_88`.

3. **DATASTORES rationale + unknowns**
   Drop "matches nixpkgs 25.05" as current rationale; close or rewrite rename unknown to match dual-disable living fact.

4. **packages-and-forks module blurb**
   Dual stock paths in the one-line description.

5. **Optional residual note**
   Soften RESIDUAL agent-done "crane rustc 1.88" so it cannot be read as living.

6. **Research banner pass (optional, separate)**
   Only if operators want research not to lie after compaction: banners on `stalwart-0.16.15-stores-evidence.md` section 2 and `version-audit.md` top; dual-disable note on scaffold section 3. **Not** a full rewrite of dated audits.

7. **No code/module slice**
   Do not re-plan stateVersion bump, rustPackages_1_88 migration, or dual disabledModules implementation. Already landed.

## Out of scope

- Bumping engines (Stalwart/Arti/WebUI) or re-running a full version currency audit.
- Changing Surmount option path away from `services.stalwart-mail`.
- Replacing dual disable with following stock `services.stalwart`.
- Mass rewrite of all research tables that correctly use 25.05 as historical contrast.
- Eval of live host channel package versions beyond noting architecture-review may be wrong on 0.11.8 for **26.05** (verify when scrubbing that row).
- Git commits, flake input bumps, or product behavior changes.
- COMPACTION-PIN dual-disable row (nice-to-have only; not a false pin today).

---

**Bottom line:** Channel and crane **code** are on 26.05 / 1.95 with dual `disabledModules`. Scrub living docs that still say 25.05 as current defaults, wrong lock rev, `rust_1_88` as crane pin, singular stock-module disable, and the DATASTORES rename "unknown." Research present-tense lies are concentrated in `stalwart-0.16.15-stores-evidence.md` and `version-audit.md`; banner or leave as dated snapshots.