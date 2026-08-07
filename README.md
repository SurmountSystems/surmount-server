# Surmount Server

Hermetic NixOS foundation for Surmount mail and web services. Replaces an
offline Synology DiskStation (MailPlus + static sites) with a reproducible
stack:

- **Stalwart** - SMTP / submission / IMAP / ManageSieve / JMAP (mail engine
  pinned in `nix/packages/stalwart-mail.nix`, currently **0.16.15**;
  **RocksDB** all-role co-location **fine for now**;
  [docs/DATASTORES.md](docs/DATASTORES.md). Internal FTS good enough for now;
  Surmount builds own search product later)
- **HTTPS edge** - TLS and routing (**nginx transitional-to-delete**; target
  **Axum-first** edge + Unix domain sockets in
  [docs/EDGE_AND_TLS.md](docs/EDGE_AND_TLS.md); cert path not locked to
  ACME-only)
- **surmount-management-ui** - Rust **Axum + Leptos SSR** (invested path;
  embedded HTML bridge today): **admin first**, then **real webmail in v1**
- **Nostr product auth** - keys via host OS / other Surmount tools (Stalwart
  keeps mail credentials; Vaultwarden for human secret UX)
- **Deploy secrets (sops-nix today)** - available at NixOS activation on the
  **host** (bucket 1); **never in public git** (plain or ciphertext); planned
  **Vaultwarden** for humans (bucket 2); prefer **LUKS2** FDE (passphrase /
  initrd SSH / TPM unlock; unlock material never in git)
- **restic** - backup options (opt-in); clever RPO/RTO later
- **No Cloudflare required path** - direct DNS to the VPS
- **Arti onion/hidden services (REQUIRED)** - first-class reachability via
  Tor Project Rust Arti HS alongside clearnet; HS keys never in git
- **Single operator-chosen VPS** - size/plan open (do not invent SKUs);
  multi-host ends when the operator says

Primary domain: `surmount.systems`
Management UI: `https://services.surmount.systems`
Mail host: `mail.surmount.systems` (MX on the apex)

## Architecture

**After compaction / context loss, read this first:**
[docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md)

**Operator direction (2026-07-30):**
[docs/operator-direction.md](docs/operator-direction.md)

Peer review of what the tree is (versions, stores; see supersession banner):
[docs/architecture-review.md](docs/architecture-review.md).

Living stack map and design notes (read these before inventing a parallel design):

| Doc | Contents |
|-----|----------|
| [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md) | **Reload first** after compaction; full durable direction |
| [docs/operator-direction.md](docs/operator-direction.md) | Dated operator direction dump |
| [docs/principles.md](docs/principles.md) | Hermetic flakes, versions, security, sockets, no nginx |
| [docs/glossary.md](docs/glossary.md) | FOD, Day-1/Day-2, Fix, Arti, DataStore, ... |
| [docs/packages-and-forks.md](docs/packages-and-forks.md) | Surmount package overlay, Stalwart fork consume |
| [docs/architecture-review.md](docs/architecture-review.md) | Versions, 0.16.15 stores, assumption review |
| [docs/STACK.md](docs/STACK.md) | Layers, ownership, stores, identity, secrets layers |
| [docs/DATASTORES.md](docs/DATASTORES.md) | Data plane inventory; Stalwart storage design |
| [docs/open-choices.md](docs/open-choices.md) | Still open / proposed design notes |
| [AGENTS.md](AGENTS.md) | Agent process pins |
| [docs/research/](docs/research/) | Deep / historical evidence notes |
| [docs/SEARCH_AND_UI.md](docs/SEARCH_AND_UI.md) | JMAP search first; phases admin then webmail v1 |
| [docs/EDGE_AND_TLS.md](docs/EDGE_AND_TLS.md) | Edge/TLS; nginx delete path; Axum-first; Arti HS required |
| [docs/SECURITY.md](docs/SECURITY.md) | Threat model, FDE/LUKS2, Nostr auth notes |
| [docs/SECRETS.md](docs/SECRETS.md) | Deploy secrets vs Vaultwarden vs LUKS |
| [docs/OPS.md](docs/OPS.md) | Logs, health, scripts, self-ops |
| [docs/hygiene.md](docs/hygiene.md) | Standing engineering rules |
| [docs/fix-and-fixos.md](docs/fix-and-fixos.md) | Fix / FixOS naming and ownership ladder |
| [secrets/README.md](secrets/README.md) | deploy-secrets bootstrap (host-local; nothing secret committed) |

Short version: Stalwart owns mail + internal FTS (for now); our Rust layer
owns admin and v1 webmail with Nostr product auth; legacy sites are static
files only; traffic works direct to the VPS; deploy secrets decrypt on the
host at activation (sops-nix today) and **never** live in the public git tree.

## Quick layout

```text
flake.nix                 # nixosConfigurations, packages, checks, devShells
modules/                  # surmount.* NixOS modules
crates/management-ui/     # Axum management UI (Leptos SSR next)
nix/packages/             # Surmount package overlay (Stalwart FODs, UI)
hosts/mail-vps/           # sample host
docs/                     # architecture, security, secrets, migration, DNS, ops
scripts/                  # operator DNS/TLS/mail smoke checks
secrets/                  # docs/placeholder only (never commit secrets)
ref/                      # study-only references / submodules
tests/                    # NixOS VM smoke test
```

## Hygiene

Read [docs/hygiene.md](docs/hygiene.md) (top rule: **never secrets in git**),
[docs/principles.md](docs/principles.md), and [AGENTS.md](AGENTS.md). Short
version: pure flake.lock builds, deploy secrets on host only for activation,
NixOS + Nix + Rust (no Python product/ops, no NPM), Maildir migration is
operator-run, Stalwart owns protocols and mail search for now, Rust owns
console/webmail and Axum-first edge, Nostr product auth, small modules,
security-first, no nginx product edge, no required Cloudflare hop, operator
direction and open choices live in `docs/`.

## Hosting

- **Operator-chosen VPS** (not a default Hetzner preference). Do **not**
  invent RAM, disk, core counts, or provider plan SKUs in public docs
  (**Q-HOST-1**).
- Prefer a provider where you control **PTR/rDNS** on the sending IP.
- Prefer install paths that allow **LUKS2** root (disko + nixos-anywhere);
  see [docs/SECURITY.md](docs/SECURITY.md) and operator-direction.md.

## Apply the flake

### Develop locally

```bash
nix develop                            # puts nixfmt, just, rustc, … on PATH
just dev                               # local management console → http://127.0.0.1:8080/
just check                             # CI-style host bar: fmt --check, clippy, test
just check-ci                          # full flake checks.<system>.ci aggregate
just fmt                               # format check only (errors if dirty; no write)
just fmt-write                         # apply cargo fmt + flake nixfmt
just test                              # cargo test only
just clippy                            # clippy -D warnings
just e2e                               # local comprehensive end-to-end (hermetic)
nix build .#management-ui
```

`just dev` runs the multi-page console with demo hostnames. No VPS or Stalwart
required (Stalwart chip stays down until something answers `SURMOUNT_STALWART_URL`).
Ctrl-C stops it. Override any `SURMOUNT_*` env before invoking.

**nixfmt (Nix formatter):** the flake already ships `nixfmt-rfc-style` as
`packages.nixfmt`, `formatter`, and in `devShells.default`. Prefer the flake
over a distro package so CI and laptop match.

```bash
# one-shot (no install)
nix run .#nixfmt -- --check flake.nix
# or use the formatter output:
nix run .#formatter -- flake.nix
# temporary shell with nixfmt on PATH
nix shell .#nixfmt
# durable shell with full tooling
nix develop
# optional: pin into your user profile
nix profile install .#nixfmt
```

On Arch, you do **not** need a pacman/AUR package for day-to-day work if you
use the flake above. If you still want a system package for other repos:

```bash
# AUR (names vary; check with paru/yay search)
paru -S nixfmt          # or nixfmt-bin / nixfmt-git, depending on AUR
# or always-nix from nixpkgs (same RFC style as this flake):
nix profile install nixpkgs#nixfmt-rfc-style
```

**Host quality bar:** `just check` runs format check (no write), clippy with
warnings denied, then `cargo test` (same order as typical CI gates). **Full
flake CI:** `just check-ci` builds `checks.<system>.ci` (crane fmt/clippy/test,
module-eval contracts, nixfmt, …). Heavy mail VM / Stalwart FOD / full host
toplevel stay separate (`just check-heavy`, optional flake checks).

### Build the NixOS system (no deploy)

```bash
nixos-rebuild build --flake .#mail-vps
# or
nix build .#nixosConfigurations.mail-vps.config.system.build.toplevel
```

### First install on a VPS

Use your preferred path (`nixos-anywhere`, manual ISO + `nixos-install`,
provider NixOS image). For FDE, prefer disko + nixos-anywhere with disk
encryption keys (SECURITY.md). Then:

1. Copy or generate `hardware-configuration.nix` into `hosts/mail-vps/`.
2. Add SSH public keys in `hosts/mail-vps/configuration.nix`.
3. Set DNS A/AAAA for `mail`, `services`, and apex (see [docs/DNS.md](docs/DNS.md)).
4. Configure sops age keys ([secrets/README.md](secrets/README.md)).
5. Switch:

```bash
nixos-rebuild switch --flake .#mail-vps
```

### Safe rebuilds

```bash
# Always build before switch on mail hosts
nixos-rebuild build --flake .#mail-vps
nixos-rebuild switch --flake .#mail-vps

# Rollback if needed
nixos-rebuild --rollback switch
```

Avoid `nixos-rebuild switch` straight from an untested main when MX is live;
build + boot into a known generation first if changes are risky.

## DNS

Full checklist and sample zone: [docs/DNS.md](docs/DNS.md).

Minimum before real mail:

1. A/AAAA for `mail.surmount.systems`
2. MX for `surmount.systems` -> `mail.surmount.systems`
3. PTR matching `mail.surmount.systems`
4. SPF + DKIM + DMARC

## Mail migration (MailPlus)

Maildir is the source of truth. See [docs/MIGRATION.md](docs/MIGRATION.md).

```bash
# After accounts exist in Stalwart:
surmount-mail-import-maildir you@surmount.systems \
  /var/lib/surmount/import/maildir/.../Maildir
```

Legacy **web sites** from Synology are expected to be **static files only**
(no app-server migration; no exceptions).

## Stalwart admin bootstrap fallback

The Surmount multi-page console is the operator surface. Use Stalwart's own
admin as a **bootstrap fallback** only (directory create, first-boot):

```bash
ssh -L 8080:127.0.0.1:8080 mail-vps
# open http://127.0.0.1:8080
```

Transitional nginx may also expose `/stalwart-admin/` on
`services.surmount.systems` for bootstrap; remove or lock down once you no
longer need it. Port numbers: confirm live defaults in modules (UI vs Stalwart
HTTP split is documented in architecture-review).

**NixOS option path:** Surmount-owned `services.stalwart-mail` module for
0.16+ `config.json` (nixpkgs TOML module disabled). See
`modules/stalwart-service.nix` and `modules/mail.nix`.

## Management UI

Multi-page **Leptos SSR** operator console (Axum edge + DOGE dark theme):

| Surface | Routes |
|---------|--------|
| HTML | `/`, `/domains`, `/accounts`, `/system`, `/mail` |
| JSON | `/health`, `/api/v1/system`, `/api/v1/domains`, `/api/v1/accounts`, `/api/v1/stalwart/status` |
| Residual | `POST /api/v1/jmap` (501 honest proxy boundary); full webmail UI parked |

- Package: `nix build .#management-ui`
- Service: `surmount-management-ui.service`
- Public: `https://services.surmount.systems/` (product edge is Axum HTTPS when
  `web.enable = false`)
- Optional onion display: `SURMOUNT_ONION_URL` or `SURMOUNT_ONION_HOSTNAME_FILE`
  (never invent a live onion in tree)
- Directory: default honest empty (`source: unavailable`); hermetic `mock` or
  live `stalwart` only when explicitly configured + host token (never
  default-on)

Honest residual: Q-AUTH-1 product answers; JMAP proxy beyond 501; v1 webmail
UI; host cutover. Day-one host order: [docs/OPS.md](docs/OPS.md). Phases:
[docs/SEARCH_AND_UI.md](docs/SEARCH_AND_UI.md).

## Operator smoke scripts

```bash
./scripts/check-dns.sh
./scripts/check-tls.sh services.surmount.systems:443
./scripts/check-mail-ports.sh mail.surmount.systems
```

See [docs/OPS.md](docs/OPS.md) and [scripts/README.md](scripts/README.md).

## Checks

```bash
nix flake check
# or individually:
nix build .#checks.x86_64-linux.management-ui
nix build .#checks.x86_64-linux.mail-vps-eval
nix build .#checks.x86_64-linux.mail-vm-test   # slower
```

## Secrets

**Never secrets in git** (plain or ciphertext), including LUKS unlock
material. Deploy layer (host-local): [secrets/README.md](secrets/README.md).
Full two-bucket story (deploy secrets / Vaultwarden) plus LUKS disk encryption:
[docs/SECRETS.md](docs/SECRETS.md). Hygiene: [docs/hygiene.md](docs/hygiene.md).

## Packages and Stalwart fork

How Surmount owns packages today and how to consume
`SurmountSystems/stalwart` without agent git:
[docs/packages-and-forks.md](docs/packages-and-forks.md).

## License / ownership

Operator-owned infrastructure for Surmount Systems. Application code under
`crates/` is dedicated to the public domain under the **Unlicense**
(SPDX: `Unlicense`). See [UNLICENSE.md](UNLICENSE.md). Workspace Cargo
metadata matches (`license = "Unlicense"` in `crates/Cargo.toml`). Upstream
dependencies and vendored packages keep their own licenses (for example
Stalwart AGPL).
