# Packages and forks (how Surmount ships software today)

Plain English. No assumption that the reader already knows Fix ladder codes.
Ladder detail still lives in [fix-and-fixos.md](fix-and-fixos.md) if you
want depth.

**Last updated:** 2026-08-25 (every `nix/packages/surmount-*.nix` is `callPackage`'d in the flake overlay, packages, apps, and hermetic checks. `just` aliases only `nix run`. No leftover product `script/*.sh`.)
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
| `nix/packages/stalwart-cli.nix` | CLI pin (admin only; no Maildir import on 1.0.x) |
| `nix/packages/vandelay.nix` | Official 0.16 Maildir++ / JMAP importer-exporter |
| `nix/packages/stalwart-webui.nix` | WebUI assets FOD |
| `nix/packages/stalwart-spam-filter.nix` | Spam rules FODs |
| `nix/packages/management-ui.nix` | Surmount Axum UI (crane) |
| `nix/packages/surmount-public-site.nix` | Apex/www static site from flake input `github:SurmountSystems/site` (no NPM; operator bumps rev) |
| `nix/packages/arti-onion-service.nix` | Surmount-owned Arti **2.5.0** source build + `onion-service-service` (HS publish); distinct from stock `pkgs.arti` |
| `nix/packages/surmount-private-data.nix` | Private-data pattern scanner (crane). `nix run .#surmount-private-data`. Pre-commit + `just check-private-data` / `just private-data`. |
| `nix/packages/surmount-host-logs.nix` | Host journal status/follow (crane). `nix run .#surmount-host-logs`. `just host-logs`. Journald stays source of truth. |
| `nix/packages/surmount-shc.nix` | SHC customer user-api client (crane; rDNS PTR + tickets). `nix run .#surmount-shc`. `just rdns-shc`. No python3. |
| `nix/packages/surmount-niced-builder.nix` | Niced ssh-ng nix-daemon helper (crane). `nix run .#surmount-niced-builder`. Installed as `/etc/surmount/niced-builder` when remote builder is on. MemoryMax stays on the NixOS module. Not a mail wrapper. |
| `nix/packages/surmount-leftover-homes.nix` | Refuse leftover repo `.agents` / `.grok` homes (crane). `nix run .#surmount-leftover-homes`. Fixtures live under `crates/surmount-leftover-homes/testdata/`. |
| `nix/packages/surmount-host-probe.nix` | inxi (no TTY), btop (TTY), hybrid TLS probe, Eternal Terminal client. `nix run .#surmount-inxi-host` / `.#surmount-btop-host` / `.#surmount-tls-hybrid` / `.#surmount-et`. |
| `nix/packages/surmount-static-sites.nix` | Apex/www publish + proven DS3018xs extra vhosts. `nix run .#surmount-deploy-static-sites` / `.#surmount-sync-static-sites`. |
| `nix/packages/surmount-diskstation.nix` | AFP mount, mDNS discover, MailPlus uid copy, public-dashboard compose. `nix run .#surmount-diskstation-afp-mount` and sibling apps. |
| `nix/packages/surmount-deploy-host.nix` | Host deploy driver (crane). `nix run .#surmount-deploy-host`. |
| `nix/packages/surmount-dns-zone.nix` | Namecheap zone tool (crane). `nix run .#surmount-dns-zone`. |
| `nix/packages/surmount-domain-audit.nix` | Public DNS/mail posture audit (crane). `nix run .#surmount-domain-audit`. |
| `nix/packages/surmount-host-cutover.nix` | Host cutover pack (crane). `nix run .#surmount-host-cutover`. |
| `nix/packages/surmount-acme-namecheap.nix` | Namecheap DNS-01 hook, laptop renew, host-profile render (crane). `nix run .#surmount-laptop-renew-cert` and sibling apps. |
| `nix/packages/surmount-stalwart-ops.nix` | Stalwart operator drivers (crane). `nix run .#register-dkim`, `.#free-stalwart-public-443`, `.#point-stalwart-mail-tls`, `.#add-stalwart-token`, `.#bootstrap-stalwart-api-token`, `.#stalwart-recovery-unlock`. |
| `nix/packages/surmount-mail-import.nix` | Maildir++ import via Vandelay (crane). `nix run .#surmount-mail-import-maildir`. |
| `nix/packages/surmount-secrets-install.nix` | Domain A to Domain B install + Vaultwarden export-to-staging (crane). `nix run .#secrets-install-host` / `.#secrets-export-bw-to-staging`. |
| `nix/packages/surmount-secrets-prompt.nix` | Domain A no-echo intake (crane). `nix run .#surmount-secrets-prompt`. `just secrets-prompt`. |
| `nix/overlays.nix` | Extra pins (flake `surmountOverlay` is primary) |
| `modules/stalwart-service.nix` | 0.16+ `config.json` service (disables both stock paths: `services/mail/stalwart-mail.nix` and `services/mail/stalwart.nix`; option `services.stalwart`; unit stays `stalwart-mail.service`) |
| `modules/*.nix` | Surmount NixOS modules |
| `flake.nix` / `flake.lock` | Inputs, checks, host entrypoints |

### Reasonable leftover exceptions (not product bash)

Product ops drivers are Rust plus one `callPackage` file each. These leftovers
are **not** that class. Do not treat them as unfinished conversion, and do not
grow new `.sh` drivers next to them.

| Leftover | Why it may stay |
|----------|-----------------|
| `crates/surmount-leftover-homes/testdata/*.sh` (three files: bad mkdir relative, bad mkdir root, good mktemp) | Scanner **fixtures**. They are sample inputs the crate tests read, not programs we run. |
| `script/git-hooks/pre-commit` | Git requires a shebang file. This one only execs `surmount-private-data --staged`. It is not a product driver. |
| `script/laptop-renew/*.service` and `*.timer` | systemd units for laptop Let's Encrypt renew. The program is `nix run .#surmount-laptop-renew-cert`. |

Do **not** wrap leftover bash in `writeShellApplication`. Do **not** grow
`justfile` into a second program. Operator face is `nix run .#<app>` with a
thin just alias.

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

**Operator tools:** each tool is one `callPackage` file under
`nix/packages/`, a workspace crate, flake `packages` / `apps` / overlay
attr, and hermetic `checks` (in `checks.*.ci` when crate src is present).
`just` is a thin alias that only `nix run`s. Product `script/*.sh` drivers
are gone. This is **not** a mega `ops.nix` and **not** leftover bash copied
into `writeShellApplication`.

**SHA-1 (later DNS wave, not a Wave 1 parser):** DNS / parent DNSSEC tooling
must fail closed on DS digest type 1 (SHA-1). Do not treat leftover SHA-1
parent records as acceptable. Wave 1 does not add a DNS parser. Living
policy: [DNS.md](DNS.md), [SECURITY.md](SECURITY.md).

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
