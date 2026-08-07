# Join: Surmount Server foundation

Date: 2026-07-30
Workspace: `/home/hunter/Projects/surmount/surmount-server`
No git commit (operator-owned).

## What was created

Hermetic NixOS + Rust foundation to replace offline Synology MailPlus:

| Area | Path | Notes |
|------|------|--------|
| Flake | `flake.nix`, `flake.lock` | nixpkgs 25.05, crane, sops-nix |
| Modules | `modules/*.nix` | `surmount.*` options; mail, web, UI, secrets, net, hardening, backups |
| Host | `hosts/mail-vps/configuration.nix` | sample with operator TODOs |
| Rust UI | `crates/management-ui` | Axum skeleton, embedded HTML, API stubs |
| Crane pkg | `nix/packages/management-ui.nix` | hermetic `packages.management-ui` |
| Tests | `tests/mail.nix` | NixOS VM smoke (Stalwart + UI health) |
| Docs | `docs/{hygiene,MIGRATION,DNS}.md`, `README.md` | full runbooks |
| Secrets | `secrets/README.md` | sops-nix bootstrap; no plaintext |
| Ref | `ref/README.md` | study-only submodules |

### Mail engine

- **Option path (25.05):** `services.stalwart-mail`
- Listeners: 25, 465, 587, 993, 4190; HTTP admin/JMAP on `127.0.0.1:8081`
- Import helper: `surmount-mail-import-maildir` (operator-run, maildir-nested)
- Spam-filter: pinned FOD `v2.0.5` spam-filter.toml (nixpkgs path is empty)

### Management UI

- Binary: `surmount-management-ui`
- systemd + nginx reverse proxy to `services.surmount.systems`
- Routes: `/`, `/health`, `/api/v1/domains`, `/api/v1/accounts`, `/api/v1/stalwart/status`, `POST /api/v1/jmap` (501)
- Stalwart `/admin` via SSH tunnel (or bootstrap `/stalwart-admin/`)

## How to apply

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

## Commands run and results

| Command | Result |
|---------|--------|
| `cargo generate-lockfile` + `cargo build -p surmount-management-ui` | OK |
| `cargo test -p surmount-management-ui` | 1 test passed |
| `nix flake lock` | OK (nixpkgs 25.05, crane, sops-nix) |
| `nix build .#management-ui` | OK (crane hermetic) |
| `nix eval` mail-vps config / firewall / stalwart enable | OK |
| `nix build .#nixosConfigurations.mail-vps.config.system.build.toplevel` | OK |
| `nix build .#checks.x86_64-linux.mail-vm-test` | OK (Stalwart + UI up; health + domains) |

## Known gaps (intentional skeleton)

1. **No real sops file yet**  -  operator creates `secrets/secrets.yaml` + `.sops.yaml`.
2. **No hardware-configuration.nix**  -  generate on VPS; sample uses qemu-guest + label root.
3. **Stalwart TLS**  -  ACME certs exist for hostnames once DNS works; wire `certificate.*` into Stalwart settings (commented TODO in `mail.nix`).
4. **Accounts**  -  declarative names only; create via admin CLI/UI + sops passwords.
5. **Spam-filter runtime**  -  rules file is FOD-pure; ASN/Geo/pyzor still want outbound DNS/HTTP at runtime (normal on a real VPS; errors in offline VM).
6. **crane warn**  -  current crane wants nixpkgs >= 26.05; builds fine on 25.05. Bump channel deliberately later.
7. **UI**  -  skeleton only; JMAP proxy and Leptos SSR are next steps (noted in README).
8. **Backups**  -  restic module opt-in; needs repo + passwordFile.
9. **DKIM**  -  generate in Stalwart after boot; publish DNS.
10. **Legacy site vhosts**  -  `surmount.web.extraVhosts` empty placeholder.

## Priority stance

Mail online correctly and maintainably over feature completeness. Skeleton UI is fine. No secrets in tree.
