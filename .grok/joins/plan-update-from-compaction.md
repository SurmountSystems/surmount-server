# Join: plan update from compaction inventory

**Date:** 2026-07-30
**Scope:** Read-only SoT inventory for session plan rewrite after compaction.
**Sources:** `docs/COMPACTION-PIN.md` (full), operator-direction, open-choices, STACK, AGENTS, arti-required join, README, tree layout.
**Do not treat older joins that said Arti was optional as law.** Living docs + COMPACTION-PIN win.

---

## Survived compaction?

**Yes.** Evidence:

- `docs/COMPACTION-PIN.md` exists, last updated 2026-07-30, labeled single reload SoT after context loss.
- Section 7 and absolute hygiene state **Arti onion/hidden services REQUIRED** (Tor Project Rust Arti); not optional; not clearnet edge replacement; HS keys never in git.
- Same language in `AGENTS.md` (Edge and security), `README.md` (bullet + Architecture table first row = COMPACTION-PIN), `docs/STACK.md`, `docs/open-choices.md` directed table, `docs/operator-direction.md` section 5.
- Join `.grok/joins/arti-required-compaction-pin.md` documents the optional-to-REQUIRED correction pass and lists living docs updated.
- README Architecture table leads with COMPACTION-PIN as reload-first.

Plan author: read this join + COMPACTION-PIN section headers; open one child doc only as needed.

---

## Product identity (1 para)

**Surmount Server** is a hermetic **NixOS + Nix + Rust** mail and web stack that replaces an offline Synology DiskStation (MailPlus + static sites) with a rebuildable flake-owned single VPS. Primary domain `surmount.systems`; management UI `services.surmount.systems`; MX `mail.surmount.systems`. **Stalwart** (overlay pin **0.16.15** binary FOD) owns SMTP/submission/IMAP/ManageSieve/JMAP, spam path, and message store (**RocksDB** all-role co-location fine for now; internal FTS good enough now). Surmount owns product admin then v1 webmail (**Axum + Leptos SSR** path; embedded HTML bridge today), **Nostr** product auth, **Axum-first** HTTPS edge (nginx transitional-to-delete), deploy secrets at activation, planned **Vaultwarden** for humans, **LUKS2** disk posture, and **Arti HS required** alongside clearnet. No Cloudflare products on critical path. Tree is public-domain aware: zero secrets in git. Form factor: one operator-chosen VPS (~16 GB / 2 TB NVMe / 16 cores) until the operator says otherwise. Naming ladder: Facta Non Verba / Fix / FixOS / upstream nixpkgs / Surmount package overlay / future fixpkgs (not shipping yet).

---

## Absolute hygiene (bullets)

- **NEVER secrets in git** (plain or ciphertext): no age/sops private keys, no LUKS unlock, no `.env`. Never suggest "encrypted in git." Host/operator channels only.
- **Stack language:** NixOS + Nix + Rust. No Python product/ops (fail2ban transitional-at-most). No NPM.
- **No nginx product edge** (in-tree transitional-to-delete). Target first-party Axum HTTPS edge. Prefer UDS local hops.
- **No Cloudflare products** as critical path (research blog cites OK for learning only).
- **Arti HS REQUIRED** next to clearnet; HS material = deploy secrets + VW human notes; never git.
- Agents **never** `git commit` / push; never touch `SurmountSystems/*` fork git unless operator explicitly orders.
- Always re-validate version pins when touching packages. Prove "unsafe" with evidence only.
- No ADR jargon. ASCII docs. No assumed operator acceptance (proposed / scaffold / research / operator direction YYYY-MM-DD).
- Hierarchical subagents; joins under `.grok/joins/`. Global **Q-*** ids. Additive "also" does not kill healthy in-flight work.

---

## Directed decisions (table: topic | pin | doc ref)

| Topic | Pin | Doc ref |
|-------|-----|---------|
| Host form | Single operator-chosen VPS; ~16 GB / 2 TB NVMe / 16 cores; not Hetzner-as-default; do not invent provider | COMPACTION-PIN s4; operator-direction s1 |
| Topology end | When operator says; no invented scale-out | STACK; operator-direction s1 |
| Mail engine | Stalwart; no parallel MTA/mailbox corpus | STACK ownership; open-choices |
| Stores now | RocksDB all-roles co-located OK | operator-direction s2; DATASTORES |
| Search now / later | Internal FTS good enough; Surmount own search later; no ES/Meilisearch convenience | operator-direction s2; SEARCH_AND_UI |
| Blob tiering / clever RPO-RTO | Not now | operator-direction s2 |
| poolWorkers | Leave upstream default (16 on directed box) | operator-direction s9 Q6 |
| Edge | No nginx product; Axum-first HTTPS; UDS preferred | operator-direction s4; EDGE_AND_TLS |
| TLS lean | :80->:443; no SSLv3/1.0/1.1; prefer TLS 1.3; hybrid PQ KEX first-class on rustls | operator-direction s4 |
| PQConnect | Separate path-layer PQ; evaluate/plan; not TLS substitute alone | research/pqconnect-and-pqc |
| Cert lock | Not locked ACME-only | operator-direction s9 Q5 |
| Access control | Merciless ban + whitelist + last-used; edge rate limits; long-term Rust+nft | operator-direction s4; research/access-control |
| Arti | HS reachability **REQUIRED**; not optional; not clearnet replacement | COMPACTION-PIN s7 |
| No CF products | Direct-to-VPS critical path | hygiene; EDGE_AND_TLS |
| Product auth | Nostr keys; OS/Surmount tool key mgmt | operator-direction s5 |
| Human secrets | Vaultwarden planned (PM API; not Bitwarden SM API) | SECRETS; research/arti-and-secrets-manager |
| Deploy secrets | Need at activation real; sops-nix scaffold; tool can change; never in public git | SECRETS; COMPACTION-PIN s8 |
| Disk | LUKS2 first-class; unlock passphrase/initrd SSH/TPM; sops does not unlock LUKS | research/luks2-and-deploy-secrets |
| UI stack | Axum + Leptos SSR invested; admin then real webmail v1 via JMAP | SEARCH_AND_UI; STACK |
| Legacy sites | Static files only, no exceptions | operator-direction s6 |
| Spam / integrity | First-class | operator-direction s6 |
| Languages | Nix + Rust only product/ops deps | principles 5b; AGENTS |
| Stalwart packaging | Overlay FODs; SurmountSystems/stalwart flake input when patches needed | packages-and-forks |
| Agents + forks | Never commit/push forks unless explicitly ordered | AGENTS; packages-and-forks |
| Migration | MailPlus Maildir operator-run only; never silent activation import | MIGRATION |
| Multi-host | Deferred | STACK |

---

## Still open Q-* (list ids + one line each)

From COMPACTION-PIN s12 / open-choices / operator-direction s10:

| Id | One line |
|----|----------|
| **Q-HOST-1** | Exact VPS provider and plan SKU (do not invent names); PTR confirmed? |
| **Q-HOST-2** | LUKS2 from day one vs interim plain disk then reinstall |
| **Q-LUKS-1** | Unlock pattern (human initrd SSH vs weaker host-local keyfile-on-boot); never from git |
| **Q-LUKS-2** | (same family) human initrd vs keyfile-on-boot detail |
| **Q-LUKS-3** | Custom initrd age decrypt later? Default lean no |
| **Q-LUKS-4** | Full root vs data-partition-only encryption |
| **Q-TLS-1+** | Cert trust path browser HTTPS vs mail TLS; DANE timing (research open) |
| **Q-CA-1** | Primary public CA at cutover |
| **Q-CA-2** | Dual-ACME / multi-CA failover on one VPS worth it? |
| **Q-PQC-1** | PQConnect Day-1 vs Day-2 |
| **Q-PQC-2** | Who runs PQConnect client |
| **Q-PQC-3** | Mail-plane PQ path mix (Stalwart / PQConnect / DANE) |
| **Q-PQC-4** | Package PQConnect overlay now or wait |
| **Q-ARTI-1** | ~~Enable Arti?~~ **Answered: REQUIRED** (do not re-open) |
| **Q-ARTI-2** | Which surfaces first over onion (private HTTP, mail, phased set) |
| **Q-ARTI-3** | Onion expose Stalwart admin/JMAP? Default lean no |
| **Q-ARTI-4** | Provider ToS/abuse if non-exit relay shares MX IP (relay optional; HS required) |
| **Q-SEC-SM-1** | Need true Bitwarden SM API vs deploy secrets + VW PM enough? |
| **Q-SEC-SM-2** | If SM required self-host: wait VW / commercial / Surmount Rust service? |
| **Q-ACL-1** | Exact "unauthorized" signals for immediate ban |
| **Q-ACL-2** | Whitelist Nix-only vs mutable + last-used store |
| **Q-ACL-3** | Ban duration default |
| **Q-ACL-4** | Rust surmount-guard timing vs fail2ban through mail live |
| **Q-ACL-5** | IPv6 ban granularity |
| **Q-ACL-6** | Mail AUTH failures: global ban vs Stalwart-local only |
| **Q-EDGE-1** | Shared host ACME PEM for mail+web vs in-process web issuance |
| **Q-EDGE-2** | UDS path layout `/run/surmount/*.sock` vs per-service |
| **Q-AUTH-1** | Session store, first-operator bootstrap allowlist, key-loss recovery |
| **Q-DEP-1** | Keep sops-nix long-term vs other deploy-secrets tool (need stays; git forbidden) |

Also open (not always Q-id): binary FOD vs require source build before live MX; system `pkgs.rocksdb` on source path; Fix/FixOS ladder as working direction not full product law.

---

## Implemented in tree vs docs-only (honest split)

### In tree (code / Nix / scaffold)

| Area | Paths / notes |
|------|----------------|
| Flake entry | `flake.nix`, `flake.lock` (nixos-25.05 channel lock) |
| Surmount package overlay | `nix/overlays.nix`, `nix/packages/`: `stalwart-mail.nix` (0.16.15 FOD), `stalwart-cli.nix`, `stalwart-webui.nix`, `stalwart-spam-filter.nix`, `management-ui.nix` |
| NixOS modules | `modules/`: `default.nix`, `options.nix`, `mail.nix`, `stalwart-service.nix`, `web.nix` (nginx transitional), `management-ui.nix`, `networking.nix`, `hardening.nix`, `secrets.nix`, `backups.nix` |
| Sample host | `hosts/mail-vps/configuration.nix` |
| Rust workspace | `crates/Cargo.toml`, `crates/management-ui/` (Axum + embedded HTML skeleton: `main.rs`, `api.rs`, `config.rs`, `pages.rs`) |
| Secrets docs only | `secrets/README.md` (no real secrets) |
| Operator scripts | `scripts/check-dns.sh`, `check-mail-ports.sh`, `check-tls.sh` |
| VM smoke | `tests/mail.nix` |
| Living + research docs | full `docs/` tree (see tree list below) |
| Study refs | `ref/README.md` |

### Docs / direction only (explicit non-claims; COMPACTION-PIN s13)

| Gap | Reality |
|-----|---------|
| Arti NixOS module / onion publish | **Not implemented** |
| Axum HTTPS edge cutover | nginx still in `modules/web.nix` |
| Merciless ban + whitelist product | Research + hardening header; fail2ban light sketch |
| Leptos SSR admin/webmail | management-ui Axum + embedded HTML only |
| Nostr auth end-to-end | Direction; verify/session not complete |
| Vaultwarden module | Planned; not wired |
| LUKS install on live VPS | Posture docs; provider + path open |
| Stalwart flake input to fork | Pattern documented; binary FOD still primary |
| PQConnect in surmount-server flake | Sibling packaging uncommitted; not wired |
| System rocksdb link | Design goal; binary embeds RocksDB |
| UDS to Stalwart HTTP | Engine IP:port; loopback scaffold |
| Production MailPlus import | Runbook + helpers; not proven on live corpus |
| Own search product | Future |
| Multi-host | Deferred |

---

## Suggested plan phases (ordered next implementation work)

Implementation-oriented residual, not research-only. Operator gates stay parked as Q-*.

1. **Keep mail foundation honest and deployable**
   Stalwart service module + FODs stay green; optional Nix knobs only if needed; no nginx expansion; hermetic checks/VM smoke stay useful. Wire real deploy-secrets path on host (sops scaffold -> real host material; never git).

2. **Axum-first HTTPS edge cutover**
   Introduce edge (in-process management-ui or small `surmount-edge` crate): TLS 1.3 lean, :80 redirect, rate limits, vhost routing to UI + Stalwart bootstrap, prefer UDS. Delete path for nginx in `modules/web.nix`. Cert path implementable without locking ACME-only forever (Q-EDGE-1 / Q-CA still open for product choice).

3. **Arti HS module (required product bar)**
   NixOS Arti packaging + HS publish for chosen surfaces (default private HTTP first unless operator answers Q-ARTI-2). HS keys via deploy secrets; document VW inventory. Do not block on optional relay (Q-ARTI-4).

4. **Access control lean**
   Edge rate limits + progress toward merciless ban/whitelist (nft + Rust); shrink Python fail2ban identity. Q-ACL-* can default-lean without full operator answers where research already leans.

5. **Management UI: admin console depth**
   Grow Axum APIs; land Leptos SSR path; Nostr session skeleton toward Q-AUTH-1; JMAP proxy beyond 501; Stalwart `/admin` remains bootstrap only.

6. **v1 webmail via JMAP**
   After admin usable; internal FTS only; no ES/Meilisearch.

7. **Mail earn-trust + operator migration**
   DNS checklist automation helpers; certs on mail ports; operator-run MailPlus Maildir import when host exists (Q-HOST-*).

8. **Later / deferred tracks**
   Vaultwarden module; PQConnect package wire after human sibling commit; system rocksdb source build; Surmount search product; multi-host; Fix/fixOS ladder productization.

---

## Critical file paths

### Reload / law

- `/home/hunter/Projects/surmount/surmount-server/docs/COMPACTION-PIN.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/operator-direction.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/open-choices.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/STACK.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/hygiene.md`
- `/home/hunter/Projects/surmount/surmount-server/docs/principles.md`
- `/home/hunter/Projects/surmount/surmount-server/AGENTS.md`
- `/home/hunter/Projects/surmount/surmount-server/README.md`

### Edge / security / secrets / data

- `docs/EDGE_AND_TLS.md`, `docs/SECURITY.md`, `docs/SECRETS.md`, `docs/DATASTORES.md`
- `docs/SEARCH_AND_UI.md`, `docs/DNS.md`, `docs/MIGRATION.md`, `docs/OPS.md`
- `docs/packages-and-forks.md`, `docs/fix-and-fixos.md`, `docs/glossary.md`
- `docs/research/arti-and-secrets-manager.md`, `rust-edge-and-uds.md`, `luks2-and-deploy-secrets.md`, `access-control-fail2ban.md`, `pqconnect-and-pqc.md`, `stalwart-0.16.15-stores-evidence.md`, `version-audit.md`

### Tree layout (inventory)

**nix/packages/**
`management-ui.nix`, `stalwart-cli.nix`, `stalwart-mail.nix`, `stalwart-spam-filter.nix`, `stalwart-webui.nix` (+ `nix/overlays.nix`)

**modules/** (repo root, not under nix/)
`backups.nix`, `default.nix`, `hardening.nix`, `mail.nix`, `management-ui.nix`, `networking.nix`, `options.nix`, `secrets.nix`, `stalwart-service.nix`, `web.nix`

**crates/**
`Cargo.toml`, `Cargo.lock`, `management-ui/{Cargo.toml,src/{main,api,config,pages}.rs}`

**docs/ top-level**
`architecture-review.md`, `COMPACTION-PIN.md`, `DATASTORES.md`, `DECISIONS.md`, `DNS.md`, `EDGE_AND_TLS.md`, `fix-and-fixos.md`, `glossary.md`, `hygiene.md`, `MIGRATION.md`, `open-choices.md`, `operator-direction.md`, `OPS.md`, `packages-and-forks.md`, `principles.md`, `SEARCH_AND_UI.md`, `SECRETS.md`, `SECURITY.md`, `STACK.md`, `research/` (12 files listed above in research bullets)

**Other**
`flake.nix`, `hosts/mail-vps/configuration.nix`, `scripts/`, `secrets/README.md`, `tests/mail.nix`, `.grok/joins/arti-required-compaction-pin.md`

---

## Plan-author note

- Status language: **operator direction 2026-07-30**, **proposed**, **scaffold default**, **research finding**, **open**. Do not say "we decided" / "locked" without explicit operator approval of that item.
- Older joins may still say Arti optional; **ignore** for product law.
- Re-verify FOD pins against upstream before packaging claims (`docs/research/version-audit.md`).
