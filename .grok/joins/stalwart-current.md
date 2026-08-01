# Join: Stalwart current pin (packaging)

**Date:** 2026-07-30
**Status:** packages + module eval green. Not operator-accepted architecture.
**Scope:** stop nixpkgs 25.05 `stalwart-mail` 0.11.8; pin current engine.

## Version pinned

| Component | Version | How packaged |
|-----------|---------|--------------|
| Stalwart server | **0.16.15** | Release binary FOD (`stalwart-{arch}-unknown-linux-gnu.tar.gz`) |
| stalwart-cli | **1.0.12** | Release binary FOD |
| WebUI assets | **1.0.7** | Release zip FOD (`webui.zip`) |
| spam-filter rules | **3.0.0** | Release toml + rules.json.gz FODs |
| nixpkgs host channel | nixos-25.05 (unchanged) | Base OS only; engine not from this channel |

Upstream latest stable tag at research time: `v0.16.15` (published 2026-07-27).
Repo: https://github.com/stalwartlabs/stalwart

## Why not stay on 0.11.8

flake input `nixos-25.05` packages `stalwart-mail` at **0.11.8**. That was a scaffold accident, not a product choice. Greenfield: no requirement to stay compatible with 0.11.8.

## Why not unstable module alone

nixpkgs-unstable (checked 2026-07-30):

- `stalwart` alias -> `stalwart_0_15` at **0.15.5** (with warning)
- `stalwart_0_16` at **0.16.14** (lagged one patch)
- Warning: `stalwart_0_16` is **not compatible** with `services.stalwart`
- Unstable module still generates **TOML** for the 0.15 world

Stalwart **0.16 broke TOML config entirely**. On-disk config is a small `config.json` that is only a `DataStore` JSON object. Everything else (listeners, accounts, spam, TLS, webui URL) is JMAP objects in the datastore, managed via WebUI or `stalwart-cli apply`. See tag `UPGRADING/v0_16.md`.

## How packaged (Surmount-owned)

### Files

| Path | Role |
|------|------|
| `nix/packages/stalwart-mail.nix` | Server 0.16.15 release binary + passthru webui/spam-filter |
| `nix/packages/stalwart-cli.nix` | CLI 1.0.12 release binary |
| `nix/packages/stalwart-webui.nix` | WebUI zip FOD |
| `nix/packages/stalwart-spam-filter.nix` | spam-filter FODs |
| `modules/stalwart-service.nix` | Surmount service module (disables nixpkgs TOML module) |
| `modules/mail.nix` | Surmount wiring: RocksDB path, FODs under `/etc/surmount/stalwart/`, helpers |
| `flake.nix` | packages + overlay (`pkgs.stalwart-mail`, `pkgs.stalwart-cli`) |

### Packaging mode: release-binary FOD (default)

```text
stalwart-mail.passthru.packagingMode = "release-binary-fod"
```

**Why not rustPlatform source build right now**

1. Attempted vendor of 0.16.15 Cargo.lock via nixpkgs `fetch-cargo-vendor`.
2. crates.io returned **HTTP 403** for multiple crate downloads from this environment (e.g. `calcard`, `azure_svc_blobstorage`).
3. CLI source build also needs rustc newer than 25.05's 1.86 (`unsigned_is_multiple_of` unstable).

Release binaries are hermetic (fixed-output hashes in the package files), match the published tag, and build in seconds. Revisit in-tree `rustPlatform.buildRustPackage` when cargo vendor works and/or rustc is new enough; keep the binary path until then.

### Flake outputs

```bash
nix build .#stalwart-mail          # GREEN 0.16.15
nix build .#stalwart-cli           # GREEN 1.0.12
nix build .#stalwart-webui         # GREEN 1.0.7
nix build .#stalwart-spam-filter   # GREEN 3.0.0
nix build .#management-ui          # GREEN (unchanged)
nix build .#checks.x86_64-linux.mail-vps-eval   # GREEN
```

## Module wiring

1. `modules/stalwart-service.nix` sets
   `disabledModules = [ "services/mail/stalwart-mail.nix" ];`
   so nixpkgs 25.05 TOML/`stalwart-mail` binary assumptions do not apply.
2. Options stay under **`services.stalwart-mail`** (same path name as 25.05) for less churn.
3. Generated **config.json**:

```json
{"@type":"RocksDb","path":"/var/lib/stalwart-mail/db"}
```

4. Unit: `stalwart-mail.service` runs
   `stalwart --config=<config.json>`
   user/group `stalwart-mail`, `CAP_NET_BIND_SERVICE`, dataDir `/var/lib/stalwart-mail`.
5. `settings` option is **accepted and ignored** (no TOML writer). Day-2 config is WebUI / `stalwart-cli apply`.
6. First boot with empty RocksDB: upstream `insert_safe_defaults` creates listeners including SMTP :25, submissions :465, IMAPS :993, ManageSieve :4190, HTTP **:8080**, HTTPS :443, POP3S :995 (binds `[::]:port`).

### Port collision fix

Surmount management UI default port moved **8080 -> 8090** so it does not fight Stalwart's first-boot HTTP :8080. nginx still proxies the UI; `/stalwart-admin/` proxies to `127.0.0.1:8080`.

## Hermetic FODs on host

Installed under `/etc/surmount/stalwart/`:

- `spam-filter.toml`
- `spam-filter-rules.json.gz`
- `webui.zip`

**Not auto-wired into the running engine.** 0.16 defaults may still point Application.resource_url / spam rules at GitHub until the operator runs WebUI or:

```bash
# After admin exists; point at hermetic files (exact object shape is schema-driven).
stalwart-cli --url http://127.0.0.1:8080 describe Application
# then create/update via apply plan with file:///etc/surmount/stalwart/webui.zip
```

Leaving impure first-boot downloads as a **known gap** until an apply plan lands.

## How to bump

### Server

1. Check https://github.com/stalwartlabs/stalwart/releases/latest
2. Edit `nix/packages/stalwart-mail.nix` `version = "X.Y.Z";`
3. Prefetch:

```bash
nix store prefetch-file \
  "https://github.com/stalwartlabs/stalwart/releases/download/vX.Y.Z/stalwart-x86_64-unknown-linux-gnu.tar.gz"
nix store prefetch-file \
  "https://github.com/stalwartlabs/stalwart/releases/download/vX.Y.Z/stalwart-aarch64-unknown-linux-gnu.tar.gz"
```

4. Paste hashes. `nix build .#stalwart-mail` and `--version`.
5. Skim `UPGRADING` in that tag for config.json / apply changes.

### CLI / WebUI / spam-filter

Same pattern: version + prefetch in the respective `nix/packages/stalwart-*.nix` files.

## Commands: green / red

| Command | Result |
|---------|--------|
| `nix build .#stalwart-mail` | **GREEN** -> 0.16.15, `stalwart --version` |
| `nix build .#stalwart-cli` | **GREEN** -> 1.0.12 |
| `nix build .#stalwart-webui` | **GREEN** |
| `nix build .#stalwart-spam-filter` | **GREEN** |
| `nix build .#management-ui` | **GREEN** |
| `nix build .#checks.x86_64-linux.mail-vps-eval` | **GREEN** (toplevel includes new unit + config.json) |
| `nix build .#stalwart-mail` via rustPlatform source | **RED** crates.io 403 on vendor (not shipped) |
| `nix build .#stalwart-cli` via rustPlatform on 25.05 rustc | **RED** unstable lib features (not shipped) |
| full `mail-vm-test` | **not run** this turn (heavy; smoke test file updated for ports) |

## Known gaps

1. **Declarative listeners / spam / webui URL** not in Nix yet; first-boot upstream defaults + operator apply. Prefer a committed `stalwart-cli apply` plan under e.g. `nix/stalwart/` next.
2. **HTTP :8080 on all interfaces** by default; rebind to loopback before production exposure.
3. **Import helper** still calls `stalwart-cli import messages ...`; new CLI is schema-driven (`get`/`query`/`apply`/...). Confirm real import path against `stalwart-cli --help` / docs before production MailPlus cutover; helper is a template.
4. **Docs tree** still mentions 0.11.8 / TOML / :8081 in places; out of scope for this packaging turn (separate plain-English cleanup agent).
5. **Source builds** blocked here by crates.io 403 and rustc age; binary FODs are the supported path until that changes.
6. **RocksDB store settings** (blobSize/bufferSize) left at upstream defaults unless set on the module.
7. No claim of operator acceptance of RocksDB co-location or listener layout; provisional Surmount wiring only.

## Store settings (re-eval for 0.16)

- Physical backend: one RocksDB at `/var/lib/stalwart-mail/db` via config.json `@type: RocksDb`.
- 0.16 no longer expresses four TOML `storage.*` role keys on disk; roles live as registry objects after boot. Conceptual four-store model still described in `docs/DATASTORES.md` (may lag wording).
- Backup still: stop service or consistent snapshot of `mailDataDir`; RocksDB single-writer assumptions unchanged at a high level.
- Docs pointer for current engine: https://stalw.art/docs/ and tag `UPGRADING/v0_16.md` (not 0.11.8 TOML docs).
