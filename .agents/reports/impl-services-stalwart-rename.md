# Implement report: option rename `services.stalwart-mail` -> `services.stalwart`

**Date:** 2026-08-07
**Goal:** Align Surmount NixOS option attribute path with stock nixpkgs 26.05 naming. Pre-deploy, no soft debt on the living option path.

## Done

### Product (option rename only)

| File | Change |
|------|--------|
| `modules/stalwart-service.nix` | `options.services.stalwart`; `cfg = config.services.stalwart`; assertion / `defaultText` / comments claim stock name and dual-disable both stock modules. **Unit stays** `systemd.services.stalwart-mail`. dataDir/user/group/StateDirectory defaults unchanged (`stalwart-mail*`, `/var/lib/stalwart-mail`). package default still `pkgs.stalwart-mail`. |
| `modules/mail.nix` | Enable and package refs under `services.stalwart`. |
| `modules/options.nix` | `mailDataDir` description cross-ref -> `services.stalwart.dataDir`. |
| `modules/default.nix` | Comment: owns `services.stalwart`. |

**Not changed (by design):** package attrs, FOD file names, unit name, StateDirectory, user/group, secrets.nix `before = stalwart-mail.service`, `tests/mail.nix` unit wait, host configs (they only set `surmount.*`).

### Living docs + secrets (same turn)

| File | Change |
|------|--------|
| `README.md` | Option is `services.stalwart`; dual-disable; unit note |
| `docs/COMPACTION-PIN.md` | Stock modules row -> Surmount option `services.stalwart` |
| `docs/packages-and-forks.md` | Module inventory row |
| `docs/DATASTORES.md` | 5.1, 5.3 path table, section 10 non-claims |
| `docs/architecture-review.md` | Module table, option path row, claim matrix |
| `docs/operator-direction.md` | `services.stalwart.blobSize` / `bufferSize` |
| `secrets/README.md` | `services.stalwart.credentials` |

### Research (claims + eval recipes)

| File | Change |
|------|--------|
| `docs/research/scaffold-assumptions-inventory.md` | Section 3: option is `services.stalwart`; stock **module body** still not adopted |
| `docs/research/stalwart-0.16.15-stores-evidence.md` | Living option path + eval command |
| `docs/research/version-audit.md` | Living stock modules row |
| `docs/research/stalwart-stores-evidence-2026-07-30.md` | **Historical body left**; footer points at living option path |

## Shape after rename

```text
NixOS option:     services.stalwart.*
systemd unit:     stalwart-mail.service
state:            /var/lib/stalwart-mail  (+ db under storePath)
cache:            /var/cache/stalwart-mail
user/group:       stalwart-mail
package attr:     pkgs.stalwart-mail (flake alias packages.stalwart)
stock modules:    dual disabledModules (stalwart-mail.nix + stalwart.nix)
```

No filesystem migration. Clean break: no dual option roots / no alias of old name.

## Verification

1. **`rg -n 'services\.stalwart-mail'`** (excluding `.git` / `.agents`):

   | Hit | Why OK |
   |-----|--------|
   | `modules/stalwart-service.nix` `systemd.services.stalwart-mail` | unit name kept |
   | `docs/research/stalwart-stores-evidence-2026-07-30.md` (3 lines) | historical 0.11.8 snapshot |

   Living product option path is only `services.stalwart`.

2. **Eval (mail-vps):**
   - `config.services.stalwart.enable` -> true
   - `config.services.stalwart.storePath` -> `/var/lib/stalwart-mail/db`

3. **`just fmt-write` then `just ci`:** exit 0 (flake `checks.<system>.ci` green: nixfmt, management-ui build/test/clippy/fmt, e2e clippy/pure, aggregate).

## Compatibility note

Breaking for any out-of-tree host that still set `services.stalwart-mail.*`. In-tree hosts use `surmount.*` only; `modules/mail.nix` wires the engine. Prefer clean break (no deploy yet).

## Out of scope this pass

- Unit / user / StateDirectory rename (needs migration plan)
- Package file rename to `stalwart.nix` or dropping `pkgs.stalwart-mail`
- Adopting stock TOML module body
- DATASTORES section 5.2 conceptual TOML soft-debt (recon item 4; not option rename)
- Full `version-audit.md` body re-audit for 26.05 (separate research rewrite)
