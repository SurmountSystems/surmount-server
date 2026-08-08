# Recon: ideal 26.05 option rename and no soft debt

**Date:** 2026-08-07
**Status:** research inventory only. Not operator acceptance of the rename.
**Scope:** follow stock nixpkgs 26.05 option naming (`services.stalwart`), clean research bodies that still present 25.05 / `rustPackages_1_88` as tree truth, and reduce scaffold-accident wording to clean historical one-liners.
**Method:** tree-wide search plus full read of `modules/stalwart-service.nix` and targeted reads of modules, tests, living docs, and named research files. No product edits in this pass.

**Living tree facts (already true before this ideal work):**

| Fact | Evidence |
|------|----------|
| Host channel | `flake.nix`: `nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05"` |
| Crane / management-ui rustc | `nix/rust-toolchain.nix`: `rustPackages_1_95` |
| Sample host / tests stateVersion | `26.05` in host and `tests/*.nix` |
| Stock modules | dual `disabledModules` in `modules/stalwart-service.nix` |
| Surmount option path **today** | still `services.stalwart-mail` (does **not** follow stock rename) |
| Engine pin | Surmount FOD **0.16.15**, not channel package |

Prior scrub inventory (doc pin drift, not this rename):
`.agents/reports/plan-26.05-stale-scrub-inventory.md`

---

## Option rename scope (every file path + what to change)

### Goal

Align Surmount **NixOS option attribute path** with stock nixpkgs 26.05:

| Layer | Stock 26.05 | Surmount today | Ideal |
|-------|-------------|----------------|-------|
| Module file (nixpkgs) | `services/mail/stalwart.nix` | dual-disabled | stay dual-disabled (Surmount still owns 0.16 `config.json`) |
| Option root | `services.stalwart` | `services.stalwart-mail` | **`services.stalwart`** |
| Package attr | often `pkgs.stalwart` / older `stalwart-mail` | `pkgs.stalwart-mail` + alias `stalwart` | keep package names unless a separate cleanup wants `pkgs.stalwart` primary |
| Unit / state / user | stock varies | `stalwart-mail.service`, `/var/lib/stalwart-mail`, user `stalwart-mail` | **can stay** (see next section) |

Surmount must **still** disable both stock paths after the rename. Stock `services.stalwart` is TOML / older packaging; Surmount owns 0.16 DataStore `config.json`. The rename is **name alignment only**, not adopting the stock module body.

### Product code (must change)

| Path | What to change |
|------|----------------|
| `/home/hunter/Projects/surmount/surmount-server/modules/stalwart-service.nix` | `cfg = config.services.stalwart-mail` -> `config.services.stalwart`; `options.services.stalwart-mail = { ... }` -> `options.services.stalwart`; every `defaultText` / assertion `message` that names `services.stalwart-mail`; header comments that say Surmount keeps the old option path. Keep dual `disabledModules` for `services/mail/stalwart-mail.nix` and `services/mail/stalwart.nix`. Keep unit/user/StateDirectory defaults unless a deliberate migration is scoped separately. |
| `/home/hunter/Projects/surmount/surmount-server/modules/mail.nix` | Header comments; `config.services.stalwart-mail.package` (spam/webui); `services.stalwart-mail = { enable = true; ... }` block; `environment.systemPackages` package reference. |
| `/home/hunter/Projects/surmount/surmount-server/modules/options.nix` | Description of `surmount.mailDataDir`: "must match `services.stalwart-mail.dataDir`" -> `services.stalwart.dataDir`. |
| `/home/hunter/Projects/surmount/surmount-server/modules/default.nix` | Comment "keeps `services.stalwart-mail`" -> owns `services.stalwart` (stock still dual-disabled). |

### Product code that references **unit** name only (no option rename required)

| Path | Note |
|------|------|
| `/home/hunter/Projects/surmount/surmount-server/modules/secrets.nix` | `before = [ ... "stalwart-mail.service" ]` stays if unit stays. |
| `/home/hunter/Projects/surmount/surmount-server/tests/mail.nix` | `wait_for_unit("stalwart-mail.service")` stays if unit stays. |
| `/home/hunter/Projects/surmount/surmount-server/tests/module-eval.nix` | Overlay attr `stalwart-mail` is the **package** fake, not the NixOS option. No option path assert found. Only change if package attr is renamed. |

### Package / flake (optional, not required for option rename)

| Path | Note |
|------|------|
| `/home/hunter/Projects/surmount/surmount-server/nix/packages/stalwart-mail.nix` | File and `pname = "stalwart-mail"` can stay. Comments about channel 0.11.8 are historical packaging rationale; soft-trim separately. |
| `/home/hunter/Projects/surmount/surmount-server/flake.nix` | Already exposes `packages.stalwart-mail` and alias `stalwart`. Overlay sets both. Option rename does not force package rename. |
| `/home/hunter/Projects/surmount/surmount-server/nix/overlays.nix` | Comment mentions `stalwart-mail` package; fine. |
| `/home/hunter/Projects/surmount/surmount-server/modules/stalwart-service.nix` | `default = pkgs.stalwart-mail` can stay (overlay package name). |

Hosts do **not** set `services.stalwart-mail` directly: `hosts/mail-vps/configuration.nix` only sets `surmount.*`; `modules/mail.nix` enables the Stalwart options. No host file rewrite for the option path.

### Living docs and secrets docs (update same turn as code)

| Path | What to change |
|------|----------------|
| `/home/hunter/Projects/surmount/surmount-server/README.md` | ~250-252: "option path stays `services.stalwart-mail`" -> Surmount option is `services.stalwart`; dual-disable stock modules; own 0.16 module. |
| `/home/hunter/Projects/surmount/surmount-server/docs/COMPACTION-PIN.md` | ~88: "Surmount keeps `services.stalwart-mail`" -> Surmount option `services.stalwart`; dual-disable stock. |
| `/home/hunter/Projects/surmount/surmount-server/docs/packages-and-forks.md` | ~49: option path stays... -> `services.stalwart`. |
| `/home/hunter/Projects/surmount/surmount-server/docs/DATASTORES.md` | ~374-375, ~418, ~645-648: replace "we keep `services.stalwart-mail`" with Surmount `services.stalwart`; dual-disable both stock paths. Keep unit/path rows accurate. |
| `/home/hunter/Projects/surmount/surmount-server/docs/architecture-review.md` | ~91, ~178, ~359: option path stays... -> `services.stalwart`. Unit row can remain `stalwart-mail.service`. |
| `/home/hunter/Projects/surmount/surmount-server/docs/operator-direction.md` | ~86: `services.stalwart-mail.blobSize` / `bufferSize` -> `services.stalwart.*`. |
| `/home/hunter/Projects/surmount/surmount-server/secrets/README.md` | ~148: `services.stalwart-mail.credentials` -> `services.stalwart.credentials`. |
| `/home/hunter/Projects/surmount/surmount-server/docs/OPS.md` | Unit ops (`systemctl` / `journalctl -u stalwart-mail`) stay if unit stays. No option path today; leave unless unit changes. |
| `/home/hunter/Projects/surmount/surmount-server/docs/SECRETS.md` | Diagram node `stalwart-mail` is process identity; fine if unit/user stay. |

### Research / inventory (after code rename, update claims)

| Path | What to change |
|------|----------------|
| `/home/hunter/Projects/surmount/surmount-server/docs/research/scaffold-assumptions-inventory.md` | Section 3 claim that Surmount **does not** follow stock rename becomes false. Rewrite claim to: Surmount **option** is `services.stalwart` (matches stock attr); stock **modules** still dual-disabled; config remains Surmount 0.16 JSON. |
| `/home/hunter/Projects/surmount/surmount-server/docs/research/stalwart-0.16.15-stores-evidence.md` | Commands and tables that eval `config.services.stalwart-mail.*` -> `services.stalwart.*`. |
| `/home/hunter/Projects/surmount/surmount-server/docs/research/stalwart-stores-evidence-2026-07-30.md` | Historical only: leave option name as historical 0.11.8 truth, or one footer note "living option is `services.stalwart`". Do not rewrite the whole historical snapshot. |
| `/home/hunter/Projects/surmount/surmount-server/docs/research/version-audit.md` | Not primarily about option path; body rewrite is channel/crane (section below). |

### Grep hits that are **not** the NixOS option (do not force-rename)

These are package names, state dirs, unit names, or process labels:

- `pname` / FOD package path `nix/packages/stalwart-mail.nix`
- Default `dataDir` `/var/lib/stalwart-mail`
- `StateDirectory` / `CacheDirectory` = `stalwart-mail`
- User/group `stalwart-mail`
- Binary symlink `stalwart-mail` -> `stalwart`
- REST / UI prose "stalwart-mail" as the engine process
- `RESIDUAL.md` heavy check `stalwart-mail` FOD
- `AGENTS.md` package pin path

Changing those is a **data / ops migration**, not the stock option rename.

---

## Unit name vs option path (can unit stay `stalwart-mail.service`?)

**Yes. The unit can stay `stalwart-mail.service` after the option rename.**

Evidence from `modules/stalwart-service.nix` (current design already separates the two):

1. Header already states intent: unit stays `stalwart-mail.service` for less churn in tests and ops; binary is `stalwart`.
2. Option root is independent of systemd unit key: `options.services.stalwart-mail` vs `systemd.services.stalwart-mail`.
3. Stock nixpkgs rename is about **option attribute path** (`services.stalwart-mail` -> `services.stalwart`). Surmount does not ship the stock module, so stock unit naming is not binding product law.

**What stays if unit is unchanged:**

| Item | Current default | Why leave it |
|------|-----------------|--------------|
| Unit | `stalwart-mail.service` | tests (`tests/mail.nix`), secrets ordering, OPS muscle memory |
| `StateDirectory` | `stalwart-mail` -> `/var/lib/stalwart-mail` | live RocksDB path; restic `mailDataDir` |
| `CacheDirectory` | `stalwart-mail` -> `/var/cache/stalwart-mail` | webadmin cache docs |
| User / group | `stalwart-mail` | static user; ownership |
| `SyslogIdentifier` | `stalwart-mail` | journal filters |
| LoadCredential dir | `/run/credentials/stalwart-mail.service/` | credentials option docs |
| `surmount.mailDataDir` default | `/var/lib/stalwart-mail` | must match dataDir |

**Ideal product shape:**

```text
services.stalwart.enable = true;          # NixOS option (stock-aligned name)
systemd.services.stalwart-mail = { ... }; # unit (stable Surmount identity)
```

Optional later polish (not required for ideal rename): document the split in COMPACTION-PIN and DATASTORES in one clear sentence so agents stop treating unit name and option path as the same string.

**Do not rename the unit casually.** That would touch `StateDirectory`, credential paths, `before=` lists, VM tests, and every OPS one-liner. Only do it with an explicit migration plan for any host that already wrote state under `/var/lib/stalwart-mail`.

---

## Research files to rewrite not banner

These files already have (or need) a top banner saying the host moved to 26.05, but **body tables still speak as if 25.05 / `rustPackages_1_88` are the living tree pin**. Ideal work rewrites the body (or marks every snapshot cell as dated), not only a banner.

### 1. `docs/research/version-audit.md` (highest debt)

- **Banner (good):** superseded for host + crane; living host 26.05; living crane `rustPackages_1_95`.
- **Body still tree-present as of audit date, without enough "then" tense:**
  - Summary: crane **1.88.0** / `rustPackages_1_88`; channel gap **nixos-25.05**; "OS channel remains operator-gated (`nixos-25.05` lock)".
  - Section 2 flake inputs: nixpkgs ref **nixos-25.05** @ `ac62194c...`.
  - Section 3 entire table: Surmount pin **25.05**, local eval 25.05pre-git / rustc 1.86, "Is flake.lock behind 25.05 tip?"
  - Follow-ups: "plan move 25.05 -> 26.05" (channel move **already done**).
  - Re-run snippet still evals `rustPackages_1_88` only.
  - Evidence anchors: Crane 1.88 / 25.05 tip as if current.
- **Ideal rewrite:** either (A) archive as dated snapshot with every living row past-tense and a short "living pins" pointer table at top, or (B) re-run audit against current lock and rewrite summary + sections 2-3 + follow-ups + eval recipes for 26.05 / `rustPackages_1_95`. Prefer (B) when next packaging pass runs; until then (A) so the body cannot be misread as SoT.

### 2. `docs/research/stalwart-0.16.15-stores-evidence.md`

- **Banner (partial):** host moved to 26.05; dual-disable; keeps option path `services.stalwart-mail`.
- **Body soft-debt:**
  - Section 2 table still centers 25.05 flake input / `25.05pre-git` / channel package 0.11.8 (labeled snapshot, but long).
  - Packaging rationale "rustc newer than nixos-25.05's 1.86" without "then".
  - Eval commands use `config.services.stalwart-mail.storePath` (update after option rename).
  - File inventory still lists flake as "nixpkgs 25.05 lock".
- **Ideal rewrite:** section 2 becomes a short historical paragraph + one snapshot table labeled 2026-07-30; living channel/module facts one table pointing at COMPACTION-PIN; option path updated when rename lands; drop present-tense "the host channel was" mixed with commands that look copy-paste ready for today without dates.

### 3. `docs/research/scaffold-assumptions-inventory.md`

- **Mixed:** section 22 already claims living host **nixos-26.05** and crane wants >= 26.05 (good).
- **Still soft-debt as tree claim:**
  - Section 2: "not nixpkgs 25.05's 0.11.8", "rustc age on 25.05" as packaging story (acceptable if past tense; tighten).
  - Section 3: **Claim the tree currently implies** option path stays `services.stalwart-mail` and **does not** follow stock rename. That is true **today** and becomes the primary claim to **flip** when ideal rename lands. Also soft-debt: "Should revisit ... **No** for ... adopting stock `services.stalwart` as Surmount's path" fights the ideal rename (stock **module** no; stock **option name** yes).
- **Ideal rewrite:** section 3 claim after rename; open-question text distinguishes option name vs adopting stock module; historical 0.11.8 only in one-liners.

### 4. Other research (lighter)

| Path | Treatment |
|------|-----------|
| `docs/research/stalwart-stores-evidence-2026-07-30.md` | **Historical only** by title/status. Keep 0.11.8 / 25.05 as the measured subject. Optional one-line footer to living evidence + living option path. Do not "modernize" tables to 0.16. |
| `docs/research/pqconnect-local-packaging.md` | Impure build "against nixos-25.05" is historical packaging note; rephrase as dated if still present-tense. |
| `docs/research/pqconnect-and-pqc.md` | "not packaged in 25.05 or unstable" is package-availability evidence; date the channel check. |

---

## Living docs still soft-debt

These are **living** docs (not pure research). Ideal cleanup is short historical one-liners where 25.05 / 0.11.8 appear, plus accurate option/module wording.

### Option path still documents "keep old name" as product fact

| Path | Issue |
|------|-------|
| `README.md` | Explicitly "option path stays `services.stalwart-mail`". |
| `docs/COMPACTION-PIN.md` | Same. |
| `docs/packages-and-forks.md` | Same. |
| `docs/DATASTORES.md` section 5.1 / section 10 | Keep old option path; dual-disable correctly stated. |
| `docs/architecture-review.md` | Option path stays; dual-disable already improved in places. |
| `docs/operator-direction.md` | Option names under old path. |
| `secrets/README.md` | credentials under old path. |

### Scaffold-accident 25.05 / 0.11.8 (mostly OK; trim only if half-living)

Acceptable as **one historical sentence** (already close):

- `docs/open-choices.md` ~84: Historical 0.11.8 was a nixos-25.05 scaffold accident.
- `docs/hygiene.md` ~66-67: 0.11.8 historical scaffold accident from older host channel (pre-26.05).
- `docs/DATASTORES.md` banner ~20-22: same.
- `docs/architecture-review.md` ~97-99: "Why not stay on 0.11.8" (good history).
- `AGENTS.md`: package pin not channel 0.11.8; historical evidence link.

Soft-debt still half-living:

| Path | Issue | Ideal |
|------|-------|-------|
| `docs/DATASTORES.md` section 5.2 | Conceptual **TOML** eval dump (`[store.db]`, spam-filter v2.0.5) looks like live Surmount config. Living truth is 0.16 `config.json` DataStore only; TOML is historical / wrong shape. | Replace with generated `config.json` example; move TOML to "historical 0.11 shape" or delete. |
| `docs/DATASTORES.md` ~463 area | Rationale still framed around older channel non-legacy defaults (prior scrub already noted). | "Matches stock non-legacy RocksDB co-location defaults" without naming 25.05 as current. |
| `nix/packages/stalwart-mail.nix` header | "That channel packages 0.11.8" without "older channels / historical". Host is 26.05; stock package version on 26.05 differs. | "Older nixos-25.05 packaged 0.11.8 (scaffold accident). Host is 26.05; engine still Surmount FOD." |
| `docs/architecture-review.md` channel package row | "historical on 25.05 (was 0.11.8)" is OK if not confused with current channel package version. | Optional: eval stock on 26.05 once, or say "unused channel attr; Surmount never uses it." |
| `docs/packages-and-forks.md` ~69 | "older host channels shipped ancient 0.11.8" | Fine as one-liner. |

### Not soft-debt (correct living)

- `flake.nix`, `flake.lock` ref, `nix/rust-toolchain.nix` (`rustPackages_1_95`)
- Sample host and tests `stateVersion = "26.05"`
- Dual `disabledModules` in product module
- COMPACTION-PIN host channel + crane rows (except option path keep)
- `docs/research/scaffold-assumptions-inventory.md` section 22 host 26.05 claim

---

## Suggested implement order

1. **Code option rename (single coherent PR / implement slice)**
   - `modules/stalwart-service.nix`: option root + cfg + assertion messages + comments.
   - `modules/mail.nix`: enable/package references.
   - `modules/options.nix`: mailDataDir description.
   - `modules/default.nix`: comment.
   - **Do not** change unit, user, StateDirectory, dataDir defaults in the same slice unless migration is explicit.

2. **Verify with tree contracts**
   - `tests/module-eval.nix` (and any eval that loads mail modules).
   - Optional: `nix eval ...#nixosConfigurations.mail-vps.config.services.stalwart.enable` (new path).
   - `tests/mail.nix` only if unit left unchanged (expect still green on unit name).

3. **Living docs + secrets README (same turn as code)**
   - COMPACTION-PIN, DATASTORES 5.1/10, architecture-review option rows, packages-and-forks, README, operator-direction, secrets/README.
   - One sentence each: option = `services.stalwart`; unit/state user may remain `stalwart-mail*`; stock modules still dual-disabled.

4. **DATASTORES section 5.2 TOML soft-debt**
   - Replace conceptual TOML with real `config.json` shape for 0.16. Separate from rename if needed, but do not leave TOML as "exact store-related settings (flake eval)" for a 0.16 host.

5. **Research body rewrites (can be second slice)**
   - `version-audit.md`: archive tense or full re-audit for 26.05 / 1.95.
   - `stalwart-0.16.15-stores-evidence.md`: historical section 2; living pointers; option path after rename.
   - `scaffold-assumptions-inventory.md` section 3 flip after rename.

6. **Optional package-comment soft-trim**
   - `stalwart-mail.nix` header historical channel wording.
   - Not blocking pre-deploy if living COMPACTION-PIN is correct.

7. **Out of scope unless operator asks**
   - Renaming unit / user / `/var/lib/stalwart-mail`.
   - Renaming package file to `stalwart.nix` or dropping `pkgs.stalwart-mail`.
   - Adopting stock TOML module body.

---

## Risks (data paths, state dirs if any)

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Option rename only** breaks host fragments that set `services.stalwart-mail.*` | Low in-tree (hosts use `surmount.*` only); medium for any out-of-tree host overlays | Grep deploy secrets docs; secrets/README credentials example; document breaking rename in MIGRATION or OPS one-liner. |
| **Adopting stock module** instead of name-only rename | High | Keep dual `disabledModules`. Never import stock `stalwart.nix` body for 0.16. |
| **Renaming unit to `stalwart.service`** | High on any host with existing state | Leave unit. |
| **Changing `StateDirectory` / default dataDir** away from `stalwart-mail` | **Critical** for any machine that already wrote RocksDB under `/var/lib/stalwart-mail` | Keep defaults. `surmount.mailDataDir` and `services.stalwart.dataDir` stay `/var/lib/stalwart-mail` unless operator migrates. |
| **User/group rename** | High (chown of large mail DB) | Keep `stalwart-mail` user/group defaults. |
| **LoadCredential path** `/run/credentials/<unit>/` | Medium if unit renamed | Keep unit. |
| **secrets.nix `before = stalwart-mail.service`** | Medium if unit renamed without update | Keep unit or update both. |
| **Package attr vs option name divergence** | Low confusion | Document: option `services.stalwart`, package often still `pkgs.stalwart-mail` / flake `stalwart-mail` (alias `stalwart` exists). |
| **Docs half-updated** (code renamed, COMPACTION-PIN still "keeps stalwart-mail option") | Medium agent confusion post-compaction | Same-turn living docs (COMPACTION-PIN first). |
| **Research banners only** | Medium "lazy residual" | Rewrite version-audit and stores-evidence bodies as above. |
| **DATASTORES TOML section** left as "flake eval" | Medium wrong ops belief (TOML settings still apply) | Replace with config.json truth. |
| **Test matrix** | Low if unit stable | mail-vm-test still waits on `stalwart-mail.service`. |

**Data path summary (ideal rename, unit stable):**

```text
NixOS option:     services.stalwart.*
systemd unit:     stalwart-mail.service
state:            /var/lib/stalwart-mail  (+ db under storePath)
cache:            /var/cache/stalwart-mail
user/group:       stalwart-mail
package attr:     pkgs.stalwart-mail (optional alias pkgs.stalwart / packages.stalwart)
```

No filesystem migration is required for the option rename alone.

---

## Quick file checklist (option string `services.stalwart-mail` product surface)

**Code consumers of the option path:**

1. `modules/stalwart-service.nix` (define + cfg)
2. `modules/mail.nix` (set + read package)
3. `modules/options.nix` (description cross-ref)
4. `modules/default.nix` (comment)

**Docs / secrets that name the option path:**

5. `README.md`
6. `docs/COMPACTION-PIN.md`
7. `docs/packages-and-forks.md`
8. `docs/DATASTORES.md`
9. `docs/architecture-review.md`
10. `docs/operator-direction.md`
11. `secrets/README.md`
12. Research files listed above (claims + eval recipes)

**Unit-only (leave unless unit migration):**

- `modules/secrets.nix`, `tests/mail.nix`, `docs/OPS.md`, StateDirectory/user in `stalwart-service.nix`

---

## Bottom line

Ideal pre-deploy cleanup is three parallel products of work, not one banner:

1. **Rename Surmount option root to `services.stalwart`**, keep dual stock disable, keep unit/state/user as `stalwart-mail*` unless a separate migration is approved.
2. **Rewrite research bodies** (especially `version-audit.md`) so 25.05 / `rustPackages_1_88` cannot be read as living tree truth.
3. **Scrub living half-debt**: option path "we keep the old name" prose after rename; DATASTORES TOML "eval" section; package headers that speak of 0.11.8 as if the current channel story.

Channel and crane pins are already on 26.05 / 1.95 in product code. Residual debt is option naming alignment plus documentation honesty.
