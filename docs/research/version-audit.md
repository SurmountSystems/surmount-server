# Version pin audit (Surmount Server)

**Status:** research finding / inventory. Not operator acceptance of any bump.
**Living host + crane refresh:** 2026-08-07 (UTC). Host channel and crane
rows describe the **tree today**.
**Arti HS package refresh:** 2026-08-27 (UTC). Surmount `arti-onion-service`
pin is **2.5.1** (matches nixpkgs-rust `arti`; crates.io max_stable 2.5.1).
**Prior full package audit:** 2026-07-31 (UTC). Stalwart FOD / cargo / upstream
"latest" cells below stay labeled as that audit day unless revalidated.

**Sources:** tree pins (`flake.lock`, `flake.nix`, `nix/packages/*.nix`,
`nix/rust-toolchain.nix`, `crates/`), GitHub Releases API (2026-07-31),
crates.io API, Tor Project GitLab tags, static.rust-lang.org, local `nix eval`
of locked nixpkgs (host facts re-checked against `flake.lock` 2026-08-07).

Living pin table: [COMPACTION-PIN.md](../COMPACTION-PIN.md).

## Process: always validate latest

Operator rule for this repo:

1. **Do not trust old audit files or package comments alone.** Docs lag.
2. Before claiming a pin is current (or before bumping), **re-fetch** upstream
   latest the same day: GitHub `releases/latest` or tags, crates.io
   `max_stable_version`, and for flake inputs the channel / branch HEAD.
3. Prefer **report gaps** for packaging work. One-line FOD version+hash bumps
   are fine only when the new tag and hash are verified in the same turn.
4. Engine (Stalwart) is owned in `nix/packages/`, not taken from nixpkgs
   `pkgs.stalwart*`. OS channel bumps are a separate, deliberate decision.
5. Re-run this audit after any packaging campaign lands, or when upstream
   release notes look material.

This file mixes **living tree facts** (host channel, crane) with
**point-in-time upstream checks**. Treat every "latest" cell as **as-of the
date in that row** until revalidated.

---

## Summary

| Area | Verdict |
|------|---------|
| Host nixpkgs channel | **Living:** `nixos-26.05` @ `445d861c6d31b4af0c79d8d4be2331f762a361d7` (`flake.lock` 2026-08-07). Sample `system.stateVersion = "26.05"`. |
| Crane / management-ui rustc | **Living:** **1.95** via `rustPackages_1_95` (`nix/rust-toolchain.nix`). Leptos MSRV floor remains **>= 1.88**. Not "at latest rustc." |
| Stock Stalwart modules | **Living:** dual `disabledModules` (`services/mail/stalwart-mail.nix` + `services/mail/stalwart.nix`); Surmount option path `services.stalwart` |
| Stalwart server FOD | **0.16.15** Surmount pin (not channel package). Upstream latest re-check **2026-07-31**: still v0.16.15 |
| stalwart-cli FOD | **1.0.12** (re-check 2026-07-31: at latest) |
| WebUI FOD | **1.0.7** (re-check 2026-07-31: at latest) |
| spam-filter FOD | **3.0.0** (re-check 2026-07-31: at latest) |
| Arti (Surmount HS package) | Surmount-owned source build **2.5.1** (`arti-onion-service.nix`; re-check 2026-08-27). Stock `pkgs.arti` is channel-lagged client-default (not HS path). rustc via `nixpkgs-rust` (MSRV 1.91+) |
| crane / sops-nix flake locks | Prior audit (2026-07-30) at master HEAD; **not re-checked** 2026-08-07 |
| management-ui Cargo.lock | Prior audit (2026-07-30): mostly current; small **tokio** patch lag; intentional older **reqwest** / **tower-http** majors. **Not re-fetched** 2026-08-07 |
| RocksDB | Embedded in upstream Stalwart lock (`10.4.2` via `librocksdb-sys`); Surmount does **not** use system `pkgs.rocksdb` for the mail binary |

**Engine is not the host channel package.** Channel lag on stock `pkgs.stalwart*`
does not set Surmount's engine version. OS channel is **already** nixos-26.05
(operator direction 2026-08-07).

### Historical footnote (2026-07-31 audit only)

On the 2026-07-31 package audit day the host was still **`nixos-25.05`** @
`ac62194c...` and crane used **`rustPackages_1_88`** (rustc 1.88). That
snapshot is **not** the living tree. Do not quote those rows as present tense.

---

## 1. Stalwart family (Surmount FODs)

Packaging mode: release binary / zip / rules FODs under `nix/packages/`.
Not source-built (vendor 403 / rustc notes; see package comments and
[stalwart-0.16.15-stores-evidence.md](stalwart-0.16.15-stores-evidence.md)).

| Pin | Tree version | Where | Latest upstream | Gap? | Source |
|-----|--------------|-------|-----------------|------|--------|
| Stalwart server | **0.16.15** | `nix/packages/stalwart-mail.nix` | **v0.16.15** (2026-07-27) | **no** (re-check 2026-07-31) | https://github.com/stalwartlabs/stalwart/releases/latest |
| stalwart-cli | **1.0.12** | `nix/packages/stalwart-cli.nix` | **v1.0.12** (2026-07-28) | **no** | https://github.com/stalwartlabs/cli/releases/latest |
| WebUI zip | **1.0.7** | `nix/packages/stalwart-webui.nix` | **v1.0.7** (2026-07-30) | **no** | https://github.com/stalwartlabs/webui/releases/latest |
| spam-filter | **3.0.0** | `nix/packages/stalwart-spam-filter.nix` | **v3.0.0** (2026-04-13) | **no** | https://github.com/stalwartlabs/spam-filter/releases/latest |

### Asset URL shapes (for future bumps)

| Package | Example asset |
|---------|----------------|
| server x86_64 | `https://github.com/stalwartlabs/stalwart/releases/download/vX.Y.Z/stalwart-x86_64-unknown-linux-gnu.tar.gz` |
| server aarch64 | `.../stalwart-aarch64-unknown-linux-gnu.tar.gz` |
| cli x86_64 | `https://github.com/stalwartlabs/cli/releases/download/vX.Y.Z/stalwart-cli-x86_64-unknown-linux-gnu.tar.xz` |
| webui | `https://github.com/stalwartlabs/webui/releases/download/vX.Y.Z/webui.zip` |
| spam-filter | `.../spam-filter.toml` and `.../spam-filter-rules.json.gz` |

Bump recipe is already in each package file header (`nix store prefetch-file`).

### Comparison: nixpkgs packages (not used for engine)

Cross-channel package comparison measured **2026-07-30** (not Surmount's
engine; Surmount FODs win):

| Channel | Package path | Version | vs Surmount 0.16.15 |
|---------|--------------|---------|---------------------|
| nixos-25.05 | `pkgs/by-name/st/stalwart-mail` | **0.11.8** | far behind (historical channel lag) |
| nixos-25.11 | `pkgs/by-name/st/stalwart-mail` | **0.14.1** | behind |
| nixos-26.05 | `pkgs/by-name/st/stalwart` | **0.15.5** | behind (host channel; still not Surmount engine) |
| nixos-unstable | `pkgs/by-name/st/stalwart_0_16` | **0.16.14** | **one patch behind Surmount** |
| nixos-unstable | `pkgs/by-name/st/stalwart-cli` | **1.0.11** | one patch behind Surmount 1.0.12 |

Surmount's FODs are intentionally **ahead of** nixpkgs for the mail stack.
Do not "sync down" to channel packages.

---

## 2. Flake inputs (`flake.lock`)

| Input | Original ref | Locked rev | Notes |
|-------|--------------|------------|-------|
| **nixpkgs** | `github:NixOS/nixpkgs/nixos-26.05` | `445d861c6d31b4af0c79d8d4be2331f762a361d7` | **Living** host channel (2026-08-07). Engine not from this package set. |
| **nixpkgs-rust** | `github:NixOS/nixpkgs/nixos-unstable` | `1559d3daa3ecc813a650b79375ea61b6741b8746` | Arti MSRV / vendor path (`flake.lock`) |
| **crane** | `github:ipetkov/crane` (default branch) | `756d6d07c3818ea95d1e2cdac63fa7d02fe3e61b` | Prior HEAD check 2026-07-30; **not re-polled** 2026-08-07. Latest release tag `v0.23.4` (2026-05-17) is older than floating master tip |
| **sops-nix** | `github:Mic92/sops-nix` | `f1406619a3884cd5c47992a70b8b35c9c0fcb4c9` | Prior HEAD check 2026-07-30; **not re-polled** 2026-08-07 |

`crane` and `sops-nix` follow branch tips (not release tags). Floating inputs
need occasional `nix flake update` even when "at HEAD" on a prior audit day;
re-check HEAD on each packaging pass.

### Historical footnote: flake lock on 2026-07-31 audit day

| Input | Then | Locked rev (then) |
|-------|------|-------------------|
| nixpkgs | `nixos-25.05` | `ac62194c3917d5f474c1a844b6fd6da2db95077d` |

That lock is **not** the living tree.

---

## 3. NixOS / nixpkgs channel (host OS)

| Item | Value (living 2026-08-07) |
|------|---------------------------|
| Surmount pin | `nixos-26.05` @ `445d861c6d31b4af0c79d8d4be2331f762a361d7` |
| Sample host `stateVersion` | **26.05** (`hosts/mail-vps/configuration.nix`, module eval tests) |
| Crane wants | host channel with `rustPackages_1_95` (26.05 ships it; 1.88 set removed) |
| Engine from channel? | **no.** Engine is Surmount FOD overlay (`nix/packages/stalwart-mail.nix`) |

### Default `rustc` on channels (nixpkgs `all-packages.nix` series map)

| Channel | Default `rust` attr | Approx series | Role for Surmount |
|---------|---------------------|---------------|-------------------|
| nixos-25.05 | `rust_1_86` | **1.86** | historical host only |
| nixos-25.11 | `rust_1_91` | 1.91 | not host |
| nixos-26.05 (Surmount host) | `rust_1_95` | **1.95** | living host + crane pin |
| nixos-unstable | `rust_1_97` | 1.97 | `nixpkgs-rust` for Arti MSRV |

### Gap assessment

| Question | Answer |
|----------|--------|
| Is living host on current stable line? | **yes** for branch name: **`nixos-26.05`**. Re-poll channel tip vs lock when bumping. |
| Does OS channel control Stalwart engine version? | **no.** Engine is Surmount FOD overlay. |
| Should Surmount still plan 25.05 -> 26.05? | **Done** (operator direction 2026-08-07). Further channel bumps only with release notes + measured need. |

### Historical footnote: host on 2026-07-31 audit day

| Item | Then |
|------|------|
| Surmount pin | `nixos-25.05` @ `ac62194c...` |
| Local eval (then) | `pkgs.lib.version` = `25.05pre-git`; `pkgs.rustc.version` = **1.86.0**; `pkgs.rocksdb.version` = **10.2.1** |
| Channel tip ages (channels.nixos.org, 2026-07-30) | 25.05 last commit **2026-01-02**; 25.11 tip **2026-06-30**; **26.05 tip 2026-07-30**; unstable tip **2026-07-29** |

---

## 4. management-ui Rust workspace

Paths: `crates/Cargo.toml`, `crates/management-ui/Cargo.toml`,
`crates/Cargo.lock`. Built with crane (`nix/packages/management-ui.nix`).
Package version string: **0.1.0** (Surmount product, not upstream).

### Edition

| Pin | Value | Notes |
|-----|-------|--------|
| workspace edition | **2021** | `crates/Cargo.toml` `[workspace.package]` |
| Edition 2024 | stable since rustc 1.85 | Available on living 1.95 rustc; optional bump, not required |

### Direct workspace dependencies

Prior crates.io check **2026-07-30** (not re-fetched 2026-08-07):

| Crate | Cargo.toml range | Cargo.lock | crates.io max_stable (2026-07-30) | Gap? |
|-------|------------------|------------|-----------------------------------|------|
| axum | `0.8` | **0.8.9** | **0.8.9** | **no** |
| tokio | `1` | **1.53.0** | **1.53.1** | **yes** (patch; `cargo update -p tokio`) |
| tower-http | `0.6` | **0.6.11** | **0.7.0** (0.6 line max **0.6.11**) | **yes** vs latest major; **no** within `0.6` pin |
| serde | `1` | **1.0.229** | **1.0.229** | **no** |
| serde_json | `1` | **1.0.151** | **1.0.151** | **no** |
| reqwest | `0.12` | **0.12.28** | **0.13.4** (0.12 line max **0.12.28**) | **yes** vs latest major; **no** within `0.12` pin |
| tracing | `0.1` | **0.1.44** | **0.1.44** | **no** |
| tracing-subscriber | `0.3` | **0.3.23** | **0.3.23** | **no** |
| anyhow | `1` | **1.0.104** | **1.0.104** | **no** |

Sources: https://crates.io/crates/<name>

### Intentional vs accidental

- **tokio 1.53.0 vs 1.53.1:** accidental lock lag. Safe candidate for
  `cargo update -p tokio` without Cargo.toml change.
- **reqwest 0.12 / tower-http 0.6:** deliberate semver caps in
  `Cargo.toml`. Moving to 0.13 / 0.7 is a small API review, not a hash bump.
- Transitive lock crate inventory is large; only **direct** deps are audited
  here. Full tree refresh = `cargo update` under the existing ranges.

---

## 5. RocksDB (Stalwart lock vs system)

Surmount mail binary is a **release FOD**. It does not link
`pkgs.rocksdb`. RocksDB versions below are for ops awareness and any future
source build / backup tooling.

### Upstream Stalwart tag `v0.16.15` `Cargo.lock`

| Crate | Locked | Role |
|-------|--------|------|
| `rocksdb` | **0.24.0** | Rust bindings (`multi-threaded-cf`) |
| `librocksdb-sys` | **0.17.3+10.4.2** | Bundled RocksDB C++ **10.4.2** |

- crates.io latest `rocksdb`: **0.24.0** (matches Stalwart; as of 2026-07-30).
- `rocksdb` 0.24.0 depends on `librocksdb-sys ^0.17.3`.
- Facebook RocksDB upstream latest release: **v11.1.2** (2026-06-25)
  https://github.com/facebook/rocksdb/releases/latest
  Stalwart's bundled **10.4.2** is behind Facebook tip; that is **upstream
  Stalwart's** choice until they bump the lock.

### System `pkgs.rocksdb` (nixpkgs, unused by Surmount FOD)

Cross-channel measure **2026-07-30** (ops awareness only):

| Channel | `pkgs.rocksdb` version (then) |
|---------|-------------------------------|
| nixos-25.05 | **10.2.1** (older than Stalwart bundle 10.4.2) |
| nixos-26.05 / nixos-unstable | **10.10.1** (newer C++ than Stalwart bundle; different lineage) |

Living host is **26.05**; re-`nix eval` `pkgs.rocksdb.version` before relying
on the number above for tooling. **Do not** assume system rocksdb can replace
or "upgrade" the engine store format. Backup scripts that call `ldb` /
rocksdb tools must match the **engine's** RocksDB generation, not whatever
nixpkgs ships.

Module config (`modules/stalwart-service.nix`) only sets store type/path and
optional `blobSize` / `bufferSize`; it does not pin a system rocksdb package.

---

## 5b. Arti (HS publish package)

Packaging mode: **Surmount-owned source build** of upstream Arti from Tor
Project GitLab (`fetchFromGitLab` tag `arti-v2.5.1`), with cargo feature
`onion-service-service`. Distinct attribute `pkgs.artiOnionService` /
`packages.*.arti-onion-service`. Does **not** replace stock `pkgs.arti`.

No official multi-arch Arti release binaries (unlike Stalwart FODs), so this
is a hermetic cargo source build, not a binary FOD.

| Item | Value | Source |
|------|-------|--------|
| Surmount package version | **2.5.1** | `nix/packages/arti-onion-service.nix` |
| Source | GitLab `tpo/core/arti` tag **`arti-v2.5.1`** | package `src` |
| Cargo feature | **`onion-service-service`** (lean; not nixpkgs `full`) | package `buildFeatures` |
| Capability passthru | `surmountOnionServiceCapable = true` | package `passthru` |
| Toolchain | rustc from flake input **`nixpkgs-rust`** (nixos-unstable; MSRV **1.91+**) | `flake.nix` `mkArtiRustPlatform` |
| Vendor | `cargoDeps` from matching nixpkgs-rust `arti` (crates.io 403 workaround) | package `artiUnstable.cargoDeps` |
| Stock nixpkgs `pkgs.arti` | channel-lagged client-default (COMPACTION-PIN: often **1.4.2**); not used for HS path | host + `nixpkgs-rust` |
| Upstream crates.io max_stable | **2.5.1** (re-check 2026-08-27; crate published 2026-08-03; no 2.6) | [crates.io arti](https://crates.io/crates/arti) (accessed: 2026-08-27) |
| Gap vs upstream engine | **closed** for Surmount HS package | own pin matches 2.5.1 |

Honest limits:

- Live Tor verify remains residual regardless of pin (`unit active != onion
  published`). Package currency is not host cutover.
- Optional `just e2e` Tor row still needs service-capable `arti` on PATH /
  built package + Tor client; local temp keys != host ownership.
- Bump recipe is in the package header (version, src hash, cargoDeps handoff,
  MSRV).
- **Build proof (2026-07-31):** `nix build .#arti-onion-service` green for
  then-current **2.5.0**; cargo features include `onion-service-service`.
  **2026-08-27 pin bump** to **2.5.1** matches nixpkgs-rust `arti` (eval
  assert) and crates.io max_stable. Cargo rebuild of the HS binary is not
  this eval slice.

## 5c. rustc (management-ui vs host vs stable)

| Pin | Version | Notes |
|-----|---------|-------|
| Living host default `pkgs.rustc` | **1.95** series on nixos-26.05 | channel default `rust_1_95` |
| Crane / management-ui toolchain | **1.95** | `rustPackages_1_95` in `nix/rust-toolchain.nix` |
| Leptos MSRV floor | **>= 1.88** | product floor; channel ships 1.95 |
| Upstream stable (static.rust-lang.org) | **1.97.1** as of 2026-07-31 | not re-polled 2026-08-07 |
| Host `rustc` on a developer machine | may differ | product builds use crane pin, not host rustup |

Gap vs latest stable is **expected** until OS channel or rust-overlay moves.
Do not treat host rustc as the product pin.

### Historical footnote: crane on 2026-07-31 audit day

| Pin (then) | Version |
|------------|---------|
| nixpkgs default `pkgs.rustc` (25.05 lock) | **1.86.0** |
| Crane / management-ui | **1.88.0** via `rustPackages_1_88` |

---

## 6. Other tree notes

| Item | Pin / state | Gap notes |
|------|-------------|-----------|
| `nix/overlays.nix` | empty | no extra pins |
| management-ui product version | 0.1.0 | Surmount-owned |
| rust-overlay flake input | commented out | not active |
| modules / hosts | no independent fetchurl pins | versions come from flake packages + nixpkgs |
| Stalwart packaging mode | `release-binary-fod` | source build deferred (vendor 403 / rustc) |
| arti-onion-service | Surmount-owned source **2.5.1** + `onion-service-service` | gap vs upstream closed as of 2026-08-27; rustc via `nixpkgs-rust` (section 5b) |
| Stock Stalwart modules | dual `disabledModules` | `modules/stalwart-service.nix` |

---

## 7. Recommended follow-ups (for packaging agent; not done here)

Priority order (proposed, not accepted):

1. **Keep Stalwart FODs as-is** until a tag newer than 0.16.15 / 1.0.12 /
   1.0.7 / 3.0.0 appears. Re-check releases before every packaging PR.
2. **Arti currency:** **shipped** as Surmount-owned **2.5.1** source package
   (2026-08-27). Re-check crates.io / GitLab tags on next packaging pass;
   bump version + hashes in `arti-onion-service.nix`. Live Tor verify remains
   residual. Do not claim onion published from pin alone.
3. **OS channel:** **done** for 25.05 -> 26.05 (living host is 26.05). Further
   bumps only with release notes + measured need. May retire `nixpkgs-rust`
   if host channel rustc meets Arti MSRV without it.
4. **management-ui:** optional `cargo update -p tokio` (prior audit 1.53.1).
   Optional review of `reqwest` 0.13 and `tower-http` 0.7 when touching the UI.
5. **flake update** crane / sops-nix on a schedule even when "at HEAD" today
   (they track default branches). Re-check HEAD on next packaging pass.
6. **Do not** replace Surmount Stalwart FODs with nixpkgs `stalwart_0_16`
   (still 0.16.14 on unstable as of 2026-07-30 audit).

---

## 8. How to re-run this audit quickly

```bash
# Stalwart family latest tags
for r in stalwartlabs/stalwart stalwartlabs/cli stalwartlabs/webui stalwartlabs/spam-filter; do
  curl -sL "https://api.github.com/repos/$r/releases/latest" \
    | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d["tag_name"], d["published_at"])'
done

# Arti upstream
curl -sL -A 'surmount-audit' "https://crates.io/api/v1/crates/arti" \
  | python3 -c 'import sys,json;c=json.load(sys.stdin)["crate"];print("arti", c["max_stable_version"])'
curl -sL "https://gitlab.torproject.org/api/v4/projects/tpo%2Fcore%2Farti/repository/tags?per_page=3" \
  | python3 -c 'import sys,json; [print(t["name"]) for t in json.load(sys.stdin)]'

# rustc stable
curl -sL "https://static.rust-lang.org/dist/channel-rust-stable.toml" | rg -A1 '\[pkg\.rust\]'

# Channel tips
for ch in nixos-25.05 nixos-25.11 nixos-26.05 nixos-unstable; do
  echo -n "$ch "; curl -sL "https://channels.nixos.org/$ch/git-revision"; echo
done

# crates.io direct deps
for c in axum tokio tower-http serde serde_json reqwest tracing tracing-subscriber anyhow; do
  curl -sL -A 'surmount-audit' "https://crates.io/api/v1/crates/$c" \
    | python3 -c 'import sys,json;c=json.load(sys.stdin)["crate"];print(c["id"], c["max_stable_version"])'
done

# Locked nixpkgs facts (host channel 26.05)
nix eval --impure --expr 'let np=builtins.getFlake (toString ./.); pkgs=import np.inputs.nixpkgs {system="x86_64-linux";}; in { arti=pkgs.arti.version; rustc=pkgs.rustc.version; rustc195=pkgs.rustPackages_1_95.rustc.version; rocksdb=pkgs.rocksdb.version; nixos=pkgs.lib.version; }'
```

Update this file's **Living host + crane refresh** date and tables when
re-running. Keep reports short under `~/.agents/reports/`.

---

## 9. Evidence anchors

| Claim | Evidence |
|-------|----------|
| Living host = nixos-26.05 @ `445d861c...` | `flake.nix` input + `flake.lock` node `nixpkgs` (2026-08-07) |
| Sample stateVersion 26.05 | `hosts/mail-vps/configuration.nix`, `tests/module-eval.nix`, `tests/mail.nix` |
| Crane rustc = 1.95 / rustPackages_1_95 | `nix/rust-toolchain.nix`, `flake.nix` devShell, `COMPACTION-PIN.md` |
| Dual stock module disable | `modules/stalwart-service.nix` `disabledModules` |
| Stalwart latest = 0.16.15 | GitHub releases/latest tag `v0.16.15` published 2026-07-27 (re-check 2026-07-31) |
| CLI latest = 1.0.12 | GitHub releases/latest 2026-07-28 (re-check 2026-07-31) |
| WebUI latest = 1.0.7 | GitHub releases/latest 2026-07-30 (re-check 2026-07-31) |
| spam-filter latest = 3.0.0 | GitHub releases/latest 2026-04-13 (re-check 2026-07-31) |
| Surmount arti-onion-service = 2.5.1 | package expression 2026-08-27; cargoDeps assert vs nixpkgs-rust `arti` |
| Arti upstream = 2.5.1 | crates.io `max_stable_version` 2026-08-27; GitLab tag `arti-v2.5.1` (published 2026-08-03). No 2.6. Historical 2.5.0 packaging: 2026-07-31 |
| rustc stable = 1.97.1 | static.rust-lang.org channel-rust-stable.toml `[pkg.rust]` 2026-07-31 |
| Historical host 25.05 / crane 1.88 | 2026-07-31 audit day only (see footnotes) |
| Current stable branch name 26.05 | channels.nixos.org + nixpkgs README Hydra links for release-26.05 (2026-07-30) |
| unstable stalwart_0_16 = 0.16.14 | raw.githubusercontent.com nixos-unstable package.nix (2026-07-30) |
| rocksdb in Stalwart lock | raw Cargo.lock at tag v0.16.15 |
| Cargo.lock direct deps | `crates/Cargo.lock` parse 2026-07-30 (not re-fetched 2026-08-07) |
| crates.io max_stable (UI deps) | crates.io API 2026-07-30 with User-Agent |
| crane / sops-nix at master HEAD | GitHub commits API vs flake.lock rev (2026-07-30; not re-checked 2026-08-07) |
