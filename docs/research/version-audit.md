# Version pin audit (Surmount Server)

**Status:** research finding / inventory. Not operator acceptance of any bump.
**Audit date:** 2026-07-31 (UTC). Re-check before any packaging change.
Prior full tables: 2026-07-30. This pass re-verified network latest for
Stalwart family + Arti + rustc and refreshed the summary; cargo lock deep
table not re-fetched unless noted.
**Sources:** tree pins (`flake.lock`, `nix/packages/*.nix`, `crates/`), GitHub
Releases API, crates.io API, Tor Project GitLab tags, static.rust-lang.org,
local `nix eval` of locked nixpkgs.

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

This file is a **point-in-time** snapshot. Treat every "latest" cell as
**as-of the audit date** until revalidated.

---

## Summary

| Area | Verdict (2026-07-31) |
|------|----------------------|
| Stalwart server FOD | **At latest** (`0.16.15`; GitHub releases/latest still v0.16.15) |
| stalwart-cli FOD | **At latest** (`1.0.12`) |
| WebUI FOD | **At latest** (`1.0.7`) |
| spam-filter FOD | **At latest** (`3.0.0`) |
| Arti (Surmount HS package) | **At latest engine pin:** Surmount-owned source build **2.5.0** (`nix/packages/arti-onion-service.nix`, GitLab `arti-v2.5.0`). Stock nixpkgs `pkgs.arti` on 25.05 remains **1.4.2** client-default (not used for HS). rustc via flake input `nixpkgs-rust` (MSRV 1.91+) |
| rustc (crane management-ui) | Tree **1.88.0** (`rustPackages_1_88`); channel default **1.86.0**; upstream stable **1.97.1** (static.rust-lang.org). Intentional MSRV floor for Leptos, not "at latest rustc" |
| crane / sops-nix flake locks | Prior audit (2026-07-30) at master HEAD; **not re-checked** this pass |
| nixpkgs channel | **Gap:** pin is `nixos-25.05` (tip frozen ~2026-01-02); current stable branch is **`nixos-26.05`** (prior audit; channel tip not re-polled this pass) |
| management-ui Cargo.lock | Prior audit (2026-07-30): mostly current; small **tokio** patch lag; intentional older **reqwest** / **tower-http** majors. **Not re-fetched** this pass |
| RocksDB | Embedded in upstream Stalwart lock (`10.4.2` via `librocksdb-sys`); Surmount does **not** use system `pkgs.rocksdb` for the mail binary |

**Stalwart FODs already current this audit window.** Arti packaging currency
landed as Surmount-owned **2.5.0** source build (not full OS channel bump).
OS channel remains operator-gated (`nixos-25.05` lock).

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

| Channel | Package path | Version | vs Surmount 0.16.15 |
|---------|--------------|---------|---------------------|
| nixos-25.05 | `pkgs/by-name/st/stalwart-mail` | **0.11.8** | far behind (scaffold accident) |
| nixos-25.11 | `pkgs/by-name/st/stalwart-mail` | **0.14.1** | behind |
| nixos-26.05 | `pkgs/by-name/st/stalwart` | **0.15.5** | behind |
| nixos-unstable | `pkgs/by-name/st/stalwart_0_16` | **0.16.14** | **one patch behind Surmount** |
| nixos-unstable | `pkgs/by-name/st/stalwart-cli` | **1.0.11** | one patch behind Surmount 1.0.12 |

Surmount's FODs are intentionally **ahead of** nixpkgs for the mail stack.
Do not "sync down" to channel packages.

---

## 2. Flake inputs (`flake.lock`)

| Input | Original ref | Locked rev | Locked date (UTC) | Latest checked | Gap? | Notes |
|-------|--------------|------------|-------------------|----------------|------|-------|
| **nixpkgs** | `github:NixOS/nixpkgs/nixos-25.05` | `ac62194c3917d5f474c1a844b6fd6da2db95077d` | 2026-01-02 | Branch tip = same rev; channel `nixos-25.05` git-revision matches | **no within branch**; **yes vs current stable** | See section 3 |
| **crane** | `github:ipetkov/crane` (default branch) | `756d6d07c3818ea95d1e2cdac63fa7d02fe3e61b` | 2026-07-29 | 2026-07-30: master HEAD = same rev; **not re-polled 2026-07-31** | **prior audit only** | Latest release tag `v0.23.4` (2026-05-17) is older than floating master tip |
| **sops-nix** | `github:Mic92/sops-nix` | `f1406619a3884cd5c47992a70b8b35c9c0fcb4c9` | 2026-07-04 | 2026-07-30: master HEAD = same rev; **not re-polled 2026-07-31** | **prior audit only** | No meaningful release tags (only `assets`) |

`crane` and `sops-nix` follow branch tips (not release tags). Floating inputs
need occasional `nix flake update` even when "at HEAD" on a prior audit day;
re-check HEAD on each packaging pass. The 2026-07-31 pass did **not** re-poll
GitHub for these two inputs (summary + section 9 match).

---

## 3. NixOS / nixpkgs channel (host OS)

| Item | Value |
|------|--------|
| Surmount pin | `nixos-25.05` @ `ac62194c...` |
| Local eval | `pkgs.lib.version` = `25.05pre-git`; `pkgs.rustc.version` = **1.86.0**; `pkgs.rocksdb.version` = **10.2.1** |
| Channel tip ages (channels.nixos.org) | 25.05 last commit **2026-01-02**; 25.11 tip **2026-06-30**; **26.05 tip 2026-07-30**; unstable tip **2026-07-29** |

### Default `rustc` on channels (from nixpkgs `all-packages.nix`)

| Channel | Default `rust` attr | Approx series |
|---------|---------------------|---------------|
| nixos-25.05 (Surmount) | `rust_1_86` | **1.86** |
| nixos-25.11 | `rust_1_91` | 1.91 |
| nixos-26.05 | `rust_1_95` | 1.95 |
| nixos-unstable | `rust_1_97` | 1.97 |

### Gap assessment

| Question | Answer |
|----------|--------|
| Is flake.lock behind `nixos-25.05` tip? | **no** (at tip) |
| Is `nixos-25.05` current stable? | **no**. Active stable line is **`nixos-26.05`** (Hydra release-26.05 continuous). Intermediate **`nixos-25.11`** still gets backports. |
| Should Surmount bump OS channel this pass? | **Report only.** Channel jumps touch openssl, nginx, kernel, sops-nix compatibility, rustc for management-ui crane builds. Operator decision. Greenfield preference is current majors (`AGENTS.md`), so **26.05 is the natural target** when ready. |
| Does OS channel control Stalwart engine version? | **no.** Engine is Surmount FOD overlay. |

---

## 4. management-ui Rust workspace

Paths: `crates/Cargo.toml`, `crates/management-ui/Cargo.toml`,
`crates/Cargo.lock`. Built with crane (`nix/packages/management-ui.nix`).
Package version string: **0.1.0** (Surmount product, not upstream).

### Edition

| Pin | Value | Notes |
|-----|-------|--------|
| workspace edition | **2021** | `crates/Cargo.toml` `[workspace.package]` |
| Edition 2024 | stable since rustc 1.85 | Available on Surmount's 1.86 rustc; optional bump, not required |

### Direct workspace dependencies

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

- crates.io latest `rocksdb`: **0.24.0** (matches Stalwart).
- `rocksdb` 0.24.0 depends on `librocksdb-sys ^0.17.3`.
- Facebook RocksDB upstream latest release: **v11.1.2** (2026-06-25)
  https://github.com/facebook/rocksdb/releases/latest
  Stalwart's bundled **10.4.2** is behind Facebook tip; that is **upstream
  Stalwart's** choice until they bump the lock.

### System `pkgs.rocksdb` (nixpkgs, unused by Surmount FOD)

| Channel | `pkgs.rocksdb` version |
|---------|------------------------|
| nixos-25.05 (Surmount lock) | **10.2.1** (older than Stalwart bundle 10.4.2) |
| nixos-26.05 / nixos-unstable | **10.10.1** (newer C++ than Stalwart bundle; different lineage) |

**Do not** assume system rocksdb can replace or "upgrade" the engine store
format. Backup scripts that call `ldb` / rocksdb tools must match the
**engine's** RocksDB generation, not whatever nixpkgs ships.

Module config (`modules/stalwart-service.nix`) only sets store type/path and
optional `blobSize` / `bufferSize`; it does not pin a system rocksdb package.

---

## 5b. Arti (HS publish package)

Packaging mode: **Surmount-owned source build** of upstream Arti from Tor
Project GitLab (`fetchFromGitLab` tag `arti-v2.5.0`), with cargo feature
`onion-service-service`. Distinct attribute `pkgs.artiOnionService` /
`packages.*.arti-onion-service`. Does **not** replace stock `pkgs.arti`.

No official multi-arch Arti release binaries (unlike Stalwart FODs), so this
is a hermetic cargo source build, not a binary FOD.

| Item | Value (2026-07-31 packaging) | Source |
|------|------------------------------|--------|
| Surmount package version | **2.5.0** | `nix/packages/arti-onion-service.nix` |
| Source | GitLab `tpo/core/arti` tag **`arti-v2.5.0`** | package `src` |
| Cargo feature | **`onion-service-service`** (lean; not nixpkgs `full`) | package `buildFeatures` |
| Capability passthru | `surmountOnionServiceCapable = true` | package `passthru` |
| Toolchain | rustc from flake input **`nixpkgs-rust`** (nixos-unstable; MSRV **1.91+**) | `flake.nix` `mkArtiRustPlatform` |
| Vendor | `cargoDeps` from matching nixpkgs-rust `arti` (crates.io 403 workaround) | package `artiUnstable.cargoDeps` |
| Stock nixpkgs `pkgs.arti` (25.05 lock) | still **1.4.2** client-default | not used for HS path |
| Upstream crates.io max_stable | **2.5.0** | crates.io API (audit day) |
| Gap vs upstream engine | **closed** for Surmount HS package | own pin matches 2.5.0 |

Honest limits:

- Live Tor verify remains residual regardless of pin (`unit active != onion
  published`). Package currency is not host cutover.
- Optional `just e2e` Tor row still needs service-capable `arti` on PATH /
  built package + Tor client; local temp keys != host ownership.
- Bump recipe is in the package header (version, src hash, cargoDeps handoff,
  MSRV).
- **Build proof (2026-07-31):** `nix build .#arti-onion-service` green;
  installCheck `arti --version` reports **2.5.0**; cargo features include
  `onion-service-service` (HS tests in package check phase ran).

## 5c. rustc (management-ui vs host vs stable)

| Pin | Version | Notes |
|-----|---------|-------|
| nixpkgs default `pkgs.rustc` (25.05 lock) | **1.86.0** | Channel default |
| Crane / management-ui toolchain | **1.88.0** | `rustPackages_1_88` for Leptos 0.8 MSRV |
| Upstream stable (static.rust-lang.org) | **1.97.1** | As of 2026-07-31 channel-rust-stable |
| Host `rustc` on audit machine | **1.97.1** | Dev host only; product builds use crane pin |

Gap vs latest stable is **expected** until OS channel or rust-overlay moves.
Do not treat host rustc as the product pin.

## 6. Other tree notes

| Item | Pin / state | Gap notes |
|------|-------------|-----------|
| `nix/overlays.nix` | empty | no extra pins |
| management-ui product version | 0.1.0 | Surmount-owned |
| rust-overlay flake input | commented out | not active |
| modules / hosts | no independent fetchurl pins | versions come from flake packages + nixpkgs |
| Stalwart packaging mode | `release-binary-fod` | source build deferred (vendor 403 / rustc) |
| arti-onion-service | Surmount-owned source **2.5.0** + `onion-service-service` | gap vs upstream closed; rustc via `nixpkgs-rust` (section 5b) |

---

## 7. Recommended follow-ups (for packaging agent; not done here)

Priority order (proposed, not accepted):

1. **Keep Stalwart FODs as-is** until a tag newer than 0.16.15 / 1.0.12 /
   1.0.7 / 3.0.0 appears. Re-check releases before every packaging PR.
2. **Arti currency:** **shipped** as Surmount-owned **2.5.0** source package
   (not full OS channel bump). Re-check crates.io / GitLab tags on next
   packaging pass; bump version + hashes in `arti-onion-service.nix`. Live
   Tor verify remains residual. Do not claim onion published from pin alone.
3. **OS channel:** plan move `nixos-25.05` -> **`nixos-26.05`** (or 25.11 as
   stepping stone). Separate campaign: eval mail-vps, rebuild management-ui,
   sops-nix, rustc series, openssl. Greenfield docs already prefer current
   majors. May retire `nixpkgs-rust` if host channel rustc meets Arti MSRV.
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

# Locked nixpkgs facts (arti + rustc)
nix eval --impure --expr 'let np=builtins.getFlake (toString ./.); pkgs=import np.inputs.nixpkgs {system="x86_64-linux";}; in { arti=pkgs.arti.version; rustc=pkgs.rustc.version; rustc188=pkgs.rustPackages_1_88.rustc.version; rocksdb=pkgs.rocksdb.version; nixos=pkgs.lib.version; }'
```

Update this file's **Audit date** and tables when re-running. Keep join under
`.grok/joins/version-audit.md` short.

---

## 9. Evidence anchors

| Claim | Evidence |
|-------|----------|
| Stalwart latest = 0.16.15 | GitHub releases/latest tag `v0.16.15` published 2026-07-27 (re-check 2026-07-31) |
| CLI latest = 1.0.12 | GitHub releases/latest 2026-07-28 (re-check 2026-07-31) |
| WebUI latest = 1.0.7 | GitHub releases/latest 2026-07-30 (re-check 2026-07-31) |
| spam-filter latest = 3.0.0 | GitHub releases/latest 2026-04-13 (re-check 2026-07-31) |
| Surmount arti-onion-service = 2.5.0 | package expression + `nix eval` 2026-07-31 packaging |
| Stock nixpkgs pkgs.arti (25.05) = 1.4.2 | channel lag; not HS path |
| Arti upstream = 2.5.0 | crates.io max_stable + GitLab tag `arti-v2.5.0` 2026-06-30 |
| rustc stable = 1.97.1 | static.rust-lang.org channel-rust-stable.toml `[pkg.rust]` 2026-07-31 |
| Crane rustc = 1.88.0 / default 1.86.0 | `nix eval` rustPackages_1_88.rustc / pkgs.rustc 2026-07-31 |
| nixpkgs 25.05 lock = channel tip | channels.nixos.org/nixos-25.05/git-revision == flake.lock rev (2026-07-30) |
| Current stable branch 26.05 | channels.nixos.org + nixpkgs README Hydra links for release-26.05 (2026-07-30) |
| unstable stalwart_0_16 = 0.16.14 | raw.githubusercontent.com nixos-unstable package.nix (2026-07-30) |
| rocksdb in Stalwart lock | raw Cargo.lock at tag v0.16.15 |
| Local rocksdb 10.2.1 | `nix eval` of flake-locked nixpkgs (2026-07-30) |
| Cargo.lock direct deps | `crates/Cargo.lock` parse 2026-07-30 (not re-fetched 2026-07-31) |
| crates.io max_stable (UI deps) | crates.io API 2026-07-30 with User-Agent |
| crane / sops-nix at master HEAD | GitHub commits API vs flake.lock rev (2026-07-30; not re-checked 2026-07-31) |
