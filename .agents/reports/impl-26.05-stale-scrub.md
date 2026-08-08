# Impl report: Living 26.05 fallout scrub (docs + comments)

Date: 2026-08-07
Plan: session plan living 26.05 fallout scrub
Scope: docs/comments only; no product logic; no git commit/add; no bulk replace

## Truth applied

| Fact | Value |
|------|--------|
| Host channel | nixos-26.05 |
| flake.lock rev | `445d861c6d31b4af0c79d8d4be2331f762a361d7` |
| Crane | `rustPackages_1_95` (rustc 1.95) |
| Leptos MSRV floor | 1.88+ (kept; not claimed as living crane pin) |
| Dual `disabledModules` | `services/mail/stalwart-mail.nix` + `services/mail/stalwart.nix` |
| Surmount option path | `services.stalwart-mail` (does not follow stock rename) |

## Files edited

### Plan steps 1-7 (required + optional COMPACTION-PIN)

| Path | Change |
|------|--------|
| `docs/architecture-review.md` | Lock rev -> `445d861c...`; channel `pkgs.stalwart-mail` historical-on-25.05 label (never channel engine); dual disable in pin table, section 2.4, challenge row 4.1; CLI rustc rationale older-channel framing |
| `crates/management-ui/Cargo.toml` | Comment: MSRV 1.88+; crane `rustPackages_1_95` on nixos-26.05 (dev-dep line still "crane rustc is 1.88+" as MSRV floor only) |
| `docs/DATASTORES.md` | Rationale no longer "matches nixpkgs 25.05" as current; section 10 rename closed (known on stock 26.05; Surmount dual-disables + keeps option path) |
| `docs/packages-and-forks.md` | Module blurb: both stock paths + option path stays |
| `RESIDUAL.md` | Agent-done crane 1.88 audit line softened with living 1.95 / COMPACTION-PIN pointer |
| `docs/research/stalwart-0.16.15-stores-evidence.md` | Top host-channel banner; section 2 past-tense / snapshot + living dual-disable note |
| `docs/research/version-audit.md` | Top banner: snapshot 2026-07-31; superseded for host + crane by 26.05 / `rustPackages_1_95` |
| `docs/research/scaffold-assumptions-inventory.md` | Section 3 dual disable + `stalwart.nix` rename; section 22 left as living 26.05 |
| `docs/COMPACTION-PIN.md` | Optional pin row: dual `disabledModules` + keeps `services.stalwart-mail` |

### Extra singular-disable comments (verification completeness)

| Path | Change |
|------|--------|
| `README.md` | Option-path blurb dual-disable + keep `services.stalwart-mail` |
| `modules/default.nix` | Import comment dual-disable (comment only) |
| `modules/mail.nix` | Header comment dual-disable (comment only) |

No FOD/package/logic edits. nixfmt not touched. `just ci` not run (comments-only `.nix` headers; no eval surface change).

## Verification (`rg`)

### Living false crane pins (`rustPackages_1_88` / `rust_1_88` as current)

Living targets checked: `crates/`, `docs/*.md`, `docs/architecture-review.md`, `docs/packages-and-forks.md`, `docs/DATASTORES.md`, `docs/COMPACTION-PIN.md`, `RESIDUAL.md`.

- **Zero** living false crane pins in those paths.
- Remaining hits only under `docs/research/version-audit.md` body (bannered snapshot; body intentionally left).
- MSRV floor wording `1.88+` remains OK in workspace + management-ui comments and COMPACTION-PIN.

### architecture-review living pin rev

- Living pin is `445d861c6d31b4af0c79d8d4be2331f762a361d7`.
- `ac62194c` **not** in architecture-review.
- Remaining `ac62194c` only in research historical/snapshot tables (`version-audit`, `stalwart-0.16.15-stores-evidence` snapshot rows, `stalwart-stores-evidence-2026-07-30`, `pqconnect-and-pqc`).

### Dual stock paths in living disable wording

Named in: architecture-review (3 places), packages-and-forks, DATASTORES (section 5.1 already + section 10), COMPACTION-PIN, README, modules comments, research banners/section 3.

### DATASTORES rename unknown

- Closed. No "Whether nixpkgs will rename..." open unknown.
- States stock 26.05 rename + Surmount dual-disable + keep path.

### Research banners

- `stalwart-0.16.15-stores-evidence.md`: host channel banner + section 2 snapshot framing.
- `version-audit.md`: superseded-for-host/crane banner.
- `scaffold-assumptions-inventory.md` section 3 dual disable (section 22 unchanged living 26.05).

### Stale phrases removed from living docs

- No "Matches nixpkgs 25.05 non-legacy defaults" as current rationale.
- No living "crane uses rust_1_88".
- No singular "TOML module" as sole disable story in living product docs (research snapshot past-tense "25.05 TOML module" left where historical).

## Residual intentionally left

1. **Research body snapshot tables** in `version-audit.md` (25.05 / `rustPackages_1_88` as audit-date facts under banner). Full re-audit out of scope.
2. **Historical 0.11.8 / 25.05 scaffold-accident framing** in hygiene, open-choices, DATASTORES intro, dated research files.
3. **management-ui** dev-dep comment `crane rustc is 1.88+` = MSRV floor, not living toolchain pin.
4. **Surmount option path** remains `services.stalwart-mail` (not stock `services.stalwart`).
5. **Channel package version on 26.05** not eval'd; architecture-review labels historical-on-25.05 / Surmount never uses channel engine.
6. **pqconnect research** still cites old lock rev in packaging experiment context (not living host SoT; not in plan critical list).

## Non-goals honored

- No dual-disable reimplementation, stateVersion, or rustPackages code work.
- No engine bumps.
- No git commit/add/stage.
- No bulk find-and-replace.
- ASCII only; no em dashes.
