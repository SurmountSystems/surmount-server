# Join: Surmount Server foundation

**Living status (2026-08-07):** this join is a **foundation-day record** from
2026-07-30. It is **not** living product SoT. Do not take channel, option path,
ports, or crane warnings from the body tables below as current.

| Living fact | Where |
|-------------|--------|
| Host channel **nixos-26.05** | `flake.nix`, `flake.lock`, `docs/COMPACTION-PIN.md` |
| Crane **rustPackages_1_95** (rustc 1.95) | `nix/rust-toolchain.nix` |
| NixOS option **`services.stalwart`** (stock name; Surmount 0.16 module) | `modules/stalwart-service.nix` |
| Unit / state still **`stalwart-mail.service`**, `/var/lib/stalwart-mail` | same module |
| Stock modules dual-disabled | `stalwart-mail.nix` + `stalwart.nix` |
| Engine Surmount FOD **0.16.15** (not channel package) | `nix/packages/stalwart-mail.nix` |
| Management UI :8090; Stalwart admin/JMAP loopback :8080 (living ports) | modules + OPS |

Date of original join: 2026-07-30
Workspace: `/home/hunter/Projects/surmount/surmount-server`
No git commit (operator-owned).

## What was created (foundation day 2026-07-30)

Hermetic NixOS + Rust foundation to replace offline Synology MailPlus:

| Area | Path | Notes (as of foundation day; see living table above) |
|------|------|--------|
| Flake | `flake.nix`, `flake.lock` | **Then** nixpkgs 25.05 + crane + sops-nix; **now** nixos-26.05 |
| Modules | `modules/*.nix` | `surmount.*` options; mail, web, UI, secrets, net, hardening, backups |
| Host | `hosts/mail-vps/configuration.nix` | sample with operator TODOs |
| Rust UI | `crates/management-ui` | Axum skeleton at foundation; later Leptos SSR multi-page |
| Crane pkg | `nix/packages/management-ui.nix` | hermetic `packages.management-ui` |
| Tests | `tests/mail.nix` | NixOS VM smoke (Stalwart + UI health) |
| Docs | `docs/{hygiene,MIGRATION,DNS}.md`, `README.md` | runbooks (expanded since) |
| Secrets | `secrets/README.md` | sops-nix bootstrap; no plaintext |
| Ref | `ref/README.md` | study-only submodules |

### Mail engine (foundation day)

- **Option path then:** `services.stalwart-mail` (stock 25.05-shaped module era)
- **Living option:** `services.stalwart` (aligns stock 26.05 attr name; Surmount owns 0.16 `config.json`)
- Listeners at foundation: 25, 465, 587, 993, 4190; HTTP admin/JMAP was on loopback (port numbers moved; see OPS / modules)
- Import helper: `surmount-mail-import-maildir` (operator-run, maildir-nested)
- Spam-filter: FOD rules (pin evolved; see packages-and-forks / COMPACTION-PIN)

### Management UI (foundation day)

- Binary: `surmount-management-ui`
- At foundation: systemd + nginx reverse proxy path. Living edge direction is Axum HTTPS; nginx transitional-to-delete.
- Routes evolved; living inventory is README + COMPACTION-PIN, not this join.

## How to apply (still roughly valid)

```bash
nix develop
nix build .#management-ui
nixos-rebuild build --flake .#mail-vps
# On host after hardware-config + SSH keys + DNS + sops:
nixos-rebuild switch --flake .#mail-vps
```

DNS: `docs/DNS.md`
Maildir import: `docs/MIGRATION.md`
Secrets: `secrets/README.md`

Hosting: operator-chosen VPS with PTR control; size/plan open (Q-HOST-1).

## Commands run and results (foundation day only)

| Command | Result (2026-07-30) |
|---------|---------------------|
| `cargo generate-lockfile` + `cargo build -p surmount-management-ui` | OK |
| `cargo test -p surmount-management-ui` | tests green that day |
| `nix flake lock` | OK (**then** nixpkgs 25.05) |
| `nix build .#management-ui` | OK (crane hermetic) |
| `nix eval` mail-vps config / firewall / stalwart enable | OK |
| `nix build .#nixosConfigurations.mail-vps.config.system.build.toplevel` | OK |
| VM / mail checks | OK that day |

For **living** CI: `just ci` / `checks.<system>.ci` on nixos-26.05.

## Known gaps (foundation day; many closed or re-ranked since)

The list below is a **2026-07-30 skeleton snapshot**. Do not treat open items as
still open without checking COMPACTION-PIN / RESIDUAL.

1. **No real sops file yet** - operator creates secrets out of band.
2. **No hardware-configuration.nix** - generate on VPS.
3. **Stalwart TLS** - host cert wiring residual; see EDGE_AND_TLS / OPS.
4. **Accounts** - directory + mutations evolved; see COMPACTION-PIN.
5. **Spam-filter runtime** - rules FOD; ASN/Geo/pyzor still want outbound DNS/HTTP.
6. **crane / channel** - **closed for 26.05**: host is nixos-26.05; no crane 25.05 warning.
7. **UI** - multi-page Leptos SSR shipped since foundation; hydrate/webmail residual.
8. **Backups** - restic module opt-in.
9. **DKIM** - generate in Stalwart after boot; publish DNS.
10. **Legacy site vhosts** - `surmount.web.extraVhosts` placeholder.

## Priority stance

Mail online correctly and maintainably over feature completeness. No secrets in tree.
Living residual: `RESIDUAL.md` + `docs/COMPACTION-PIN.md`.
