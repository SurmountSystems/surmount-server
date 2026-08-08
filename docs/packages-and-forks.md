# Packages and forks (how Surmount ships software today)

Plain English. No assumption that the reader already knows Fix ladder codes.
Ladder detail still lives in [fix-and-fixos.md](fix-and-fixos.md) if you
want depth.

**Last updated:** 2026-07-31
**Operator direction:** [operator-direction.md](operator-direction.md)

---

## 1. Three names that matter

| Name | What it is | Today? |
|------|------------|--------|
| **upstream nixpkgs** | Community package set and NixOS modules. Flake input `nixpkgs` (currently **nixos-26.05**). | Yes. Base OS, kernel, many libraries, unrelated services. |
| **Surmount package overlay** | Packages and modules **we own in this repo** so critical engines are not stuck on channel lag. Lives under `nix/packages/`, wired through `nix/overlays.nix` and `flake.nix`. | Yes. Stalwart pin, CLI, webui assets, spam-filter FODs, management-ui, custom Stalwart service module. |
| **future fixpkgs channel** | A later shared Surmount package set/channel other repos can consume (same pins, one bump). | Not shipping as a separate product yet. Grow into it when multiple trees need the same overlay. |

**Fix** / **FixOS** are names for deeper ownership of Nix / NixOS lineage if
evidence says we need it. They are not required to understand day-to-day
package bumps in this repo.

```text
  upstream nixpkgs  (community)
         |
         |  overlay / flake packages
         v
  Surmount package overlay  (this repo: nix/packages + modules)
         |
         |  later, if many repos need the same pins
         v
  future fixpkgs channel  (shared Surmount set)
```

---

## 2. What we own in-tree today

| Path | Role |
|------|------|
| `nix/packages/stalwart-mail.nix` | Stalwart server pin (release binary FOD today) |
| `nix/packages/stalwart-cli.nix` | CLI pin |
| `nix/packages/stalwart-webui.nix` | WebUI assets FOD |
| `nix/packages/stalwart-spam-filter.nix` | Spam rules FODs |
| `nix/packages/management-ui.nix` | Surmount Axum UI (crane) |
| `nix/packages/arti-onion-service.nix` | Surmount-owned Arti **2.5.0** source build + `onion-service-service` (HS publish); distinct from stock `pkgs.arti` |
| `nix/overlays.nix` | Extra pins (flake `surmountOverlay` is primary) |
| `modules/stalwart-service.nix` | 0.16+ `config.json` service (disables both stock paths: `services/mail/stalwart-mail.nix` and `services/mail/stalwart.nix`; option `services.stalwart`; unit stays `stalwart-mail.service`) |
| `modules/*.nix` | Surmount NixOS modules |
| `flake.nix` / `flake.lock` | Inputs, checks, host entrypoints |

**Arti note:** stock nixpkgs `pkgs.arti` often lags (client-default, not
HS-capable). Surmount owns a **current** Arti pin (`2.5.0` from GitLab
`arti-v2.5.0`) at `nix/packages/arti-onion-service.nix` with cargo feature
`onion-service-service`, exposed as `pkgs.artiOnionService` /
`packages.*.arti-onion-service` (does **not** replace `pkgs.arti`). rustc
comes from flake input `nixpkgs-rust` when Arti MSRV exceeds the host
channel pin. Vendor crates use matching `nixpkgs-rust` `arti.cargoDeps`
with a version assert (crates.io `fetch-cargo-vendor` 403 workaround); bump
`nixpkgs-rust` until `pkgs.arti.version` matches, or restore plain `cargoHash`
when crates.io vendor works again. Module `arti-hidden-service.nix` prefers
the Surmount package when `package` is null. Cargo build is heavy and is
**not** in `checks.*.ci`; eval-only version/feature/passthru contract is.
Live Tor publish remains residual (operator HS keys + network verify). No
official multi-arch Arti release binaries, so this is source build not binary
FOD.

**Why own Stalwart here:** older host channels shipped ancient 0.11.8.
Greenfield wants **current** Stalwart. Channel lag is not a reason to ship
old mail.

**Why own Arti here:** same spirit. Channel 1.4.2 lag is not a reason to ship
old Tor for the required HS surface.

---

## 3. Packaging modes for Stalwart

### 3.1 Current: release binary FOD

- Download upstream (or fork) published `stalwart-*-unknown-linux-gnu.tar.gz`
  with a fixed hash.
- Hermetic: hash pins the bytes.
- **Gap:** the binary embeds its own RocksDB and other native code. We do
  not dynamically link `pkgs.rocksdb` in this mode. Auditing means trusting
  the upstream release build or moving to source build.

### 3.2 Goal when source build works: system RocksDB

When cargo vendor / rustPlatform works reliably under Nix:

1. Build Stalwart from a **known source rev** (upstream tag or Surmount fork).
2. Link **`pkgs.rocksdb`** (or fixpkgs rocksdb) with a known version/ABI
   rather than relying only on crates.io `librocksdb-sys` bundled sources.
3. One RocksDB to patch and audit on the host story; better determinism for
   native code we care about.

Do not delete the working binary FOD path until source build is proven in CI
and on the target host. Mark gaps honestly.

### 3.3 Spam-filter and WebUI FODs

Separate fixed-output fetches. Bump when upstream release notes require it.
Not the message database.

---

## 4. Surmount fork of Stalwart

**Directed (operator 2026-07-30 follow-up):** use
**SurmountSystems/stalwart** so Surmount can patch when needed. The fork
**must be integrated into our Nix flakes** (flake input), not left as a
forever side manual build outside the flake.

**Remote:**
`git@github.com:SurmountSystems/stalwart.git`
(HTTPS equivalent if the repo is public and SSH is undesirable for flakes.)

### Required: flake input consume path

Document the wired choice in `flake.nix` comments when the input lands.

#### Primary: flake input with pinned rev

```nix
# Illustrative only. Operator sets ref/rev and bumps deliberately.
stalwart-src = {
  url = "git+ssh://git@github.com/SurmountSystems/stalwart.git?ref=main";
  # After first lock, prefer pinning flake.lock rev; bump with:
  #   nix flake lock --update-input stalwart-src
  # For tag tracking:
  #   url = "git+ssh://git@github.com/SurmountSystems/stalwart.git?ref=refs/tags/v0.16.15";
};
```

- **Operator** bumps the input when the fork moves.
- Package expression builds from `inputs.stalwart-src` when source build is
  enabled.
- Until source build works, the fork still tracks **release tags** and we can
  keep binary FODs whose version matches the fork's release line (URLs may
  still point at upstream release assets until fork assets differ).

#### Optional aid: operator-managed path or submodule under `ref/`

```text
ref/stalwart/   # submodule or manual clone; operator maintains
```

- Prefer flake inputs over ambient paths for pure eval.
- `ref/` remains study-only for unrelated trees; if used as a build input,
  it must be intentional and locked, and still not replace flake integration
  as the long-term story.
- **Operator** updates the submodule. Agents do not `git submodule update`
  push cycles unless explicitly instructed.

#### Binary FOD until source build

1. Fork tracks/mirrors upstream tags (or Surmount-tagged releases).
2. `nix/packages/stalwart-mail.nix` keeps binary FOD URLs pointed at the
   **fork's** release assets **or** upstream assets until fork releases differ.
3. When Surmount patches matter, publish fork release assets or switch to
   source build from the fork rev via the flake input.

### Agent rules (hard)

| Agents may | Agents must not |
|------------|-----------------|
| Read package files; suggest version bumps | `git commit` / `git push` to the fork or this repo |
| Update hashes **in this repo** when the operator asked for a version bump | Open PRs to SurmountSystems/stalwart without explicit instruction |
| Document consume patterns | Assume network clone of the fork is always allowed |
| Mark binary vs source gaps | Touch fork git history "to help" |

Agents never treat fork maintenance as implied by a docs edit.

### Bump checklist (human)

1. Decide version or fork rev.
2. If binary FOD: prefetch tarballs; paste hashes into
   `nix/packages/stalwart-mail.nix` (and cli/webui/spam as needed).
3. If source input: `nix flake lock --update-input stalwart-src` (or edit rev).
4. Read upstream `UPGRADING` notes for that tag.
5. `nix build .#stalwart-mail` (and checks).
6. Update evidence notes if store/config surface changed.
7. Human-signed commit in **this** repo when ready.

---

## 5. How this relates to Fix / fixpkgs later

| Phase | What you do |
|-------|-------------|
| Now | Surmount package overlay in `surmount-server` only |
| Several hosts/repos need same pins | Extract overlay into a small flake others input (**early fixpkgs-shaped channel**) |
| Module/installer policy diverges hard | Climb FixOS ladder with evidence ([fix-and-fixos.md](fix-and-fixos.md)) |
| Evaluator/daemon policy diverges hard | Climb Fix ladder with evidence |

Do not rename day-to-day overlay work as "we forked NixOS" if we only pin
Stalwart binaries.

---

## 6. Version validation

When touching package pins:

1. Check upstream latest stable/tag for that component.
2. Prefer current majors (greenfield rule).
3. Record measured version in architecture-review / research notes when the
   pin changes meaningfully.
4. Do not leave docs saying "current" with a stale concrete number elsewhere
   without a pointer to the package file as source of truth.

Source of truth for Stalwart server version: `nix/packages/stalwart-mail.nix`
`version` field (and `flake.lock` if a src input exists).

---

## 7. Related

- [principles.md](principles.md) - hermeticity, system libs, security
- [operator-direction.md](operator-direction.md) - dated direction including fork
- [fix-and-fixos.md](fix-and-fixos.md) - ownership ladder
- [architecture-review.md](architecture-review.md) - measured pins
- `nix/packages/stalwart-mail.nix` - live package
