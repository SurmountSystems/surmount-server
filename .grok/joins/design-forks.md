# Join: design forks pinned (2026-07-30)

## Mission

Pin operator answers on UI depth, frontend, identity, multi-host, secrets
(three layers), and legacy sites into durable ADRs and stack docs. No full
Vaultwarden/LUKS/Nostr implementation this turn.

## Done

### ADRs (`docs/DECISIONS.md`)

| ADR | Status | Decision |
|-----|--------|----------|
| **004** | accepted (strengthened) | **Axum + Leptos SSR** invested path; not templates forever |
| **005** | accepted (supersedes minimal-web-forever reading) | **Admin first**, then **real webmail in v1**; desktop complementary |
| **008** | accepted (expanded) | **Three-layer secrets**: sops-nix (A), Vaultwarden (B), LUKS2 (C); sops primary; no agenix dual-stack |
| **012** | accepted direction | **Nostr-first product auth** (NIP-98 + session open details); Stalwart mail creds + bridge; Stalwart has OIDC/LDAP but not native Nostr |
| **013** | **deferred** | Multi-host / multi-node intentionally deferred; single mail VPS |
| **014** | accepted | Legacy sites **static files only** |
| **015** | accepted direction | Prefer **LUKS2** via disko/nixos-anywhere when install allows; headless unlock tradeoffs documented |

### New / updated docs

- `docs/SECURITY.md` (new) - threat sketch, FDE, sops vs VW, Nostr high level, no CF, architecture diagram
- `docs/SECRETS.md` (new) - full three-layer secrets story + anti-patterns
- `docs/STACK.md` - identity, secrets layers, static sites, multi-host deferred, admin/webmail
- `docs/SEARCH_AND_UI.md` - phases: 0 skeleton, 1 admin+Nostr, 2 Leptos admin, 3 webmail v1, 4 onboarding, 5 optional indexes
- `docs/hygiene.md`, `README.md`, `secrets/README.md`, `docs/OPS.md` - links and layer language
- Light TODOs: `modules/secrets.nix` (VW), `modules/web.nix` + `options.nix` (static sites), `crates/management-ui/src/main.rs` (Nostr auth)

### Research notes (verified enough for docs)

- **Vaultwarden:** nixpkgs `services.vaultwarden`; env-based config; reverse-proxy patterns common. Upstream https://github.com/dani-garcia/vaultwarden/
- **LUKS2 on NixOS:** first-class (`boot.initrd.luks`, wiki FDE + remote unlock, disko, nixos-anywhere `--disk-encryption-keys`). VPS images often unencrypted; TPM rare; initrd SSH unlock is the realistic remote default.
- **Stalwart external auth:** OIDC directory + LDAP/SQL supported; can be OIDC provider/client. **Not** Nostr. Bridge pattern documented.
- **NIP-98:** kind 27235, `Authorization: Nostr <base64>`, `u` + `method` tags, time window; optional payload hash.

## Open implementation (honest; not done this turn)

- Nostr session store design; first-operator npub allowlist; key-loss recovery
- Full Vaultwarden NixOS production module + hostname
- Actual VPS reinstall with LUKS2 + chosen unlock method
- nginx -> Caddy cutover (already ADR-007 elsewhere)
- Leptos SSR wiring and webmail UI

## Out of scope (honored)

- No git commit
- No plaintext secrets
- No claim VW encrypts mail/RocksDB or replaces sops
- No multi-node design work

## Key paths

- `/home/hunter/Projects/surmount/surmount-server/docs/DECISIONS.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/SECURITY.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/SECRETS.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/STACK.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/SEARCH_AND_UI.md`
- `/home/hunter/Projects/surmount/surmount-server/secrets/README.md`
