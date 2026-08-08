# Scaffold assumptions inventory

**Date:** 2026-07-30 (inventory body); host/channel rows refreshed **2026-08-07**.
**Status:** research inventory only. Not operator acceptance.
**Scope:** every material architectural assumption the Surmount Server tree
currently implies via modules, packages, docs, or joins.

**Living host (2026-08-07):** **nixos-26.05**, crane **`rustPackages_1_95`**,
dual stock Stalwart module disable. See section 3 and section 22.

**How to read this file**

| Field | Meaning |
|-------|---------|
| **Claim** | What the tree currently implies (code defaults, docs, or both) |
| **Where** | Primary paths |
| **Why it might be there** | Likely reason someone put it in (scaffold speed, upstream default, etc.) |
| **Status** | scaffold only / proposed in docs / required by working code / unknown |
| **Revisit now that Stalwart is 0.16.15?** | yes / no / why |
| **Open question** | One clear question for the operator |

Nothing below is "we decided." Prefer **proposed**, **scaffold default**,
**research finding**, **open**, **operator-deferred**.

**Companions:** [../open-choices.md](../open-choices.md), [../STACK.md](../STACK.md),
[../DATASTORES.md](../DATASTORES.md), [../fix-and-fixos.md](../fix-and-fixos.md),
[../../.grok/joins/stalwart-current.md](../../.grok/joins/stalwart-current.md).

**Hygiene correction 2026-07-30:** living law is **never secrets in git**
(plain or ciphertext) for this public repo. Rows below that still say
"secrets in git" describe the **old scaffold implication** only; see
[../hygiene.md](../hygiene.md) and [../SECRETS.md](../SECRETS.md) for current
direction. Do not treat those rows as permission to commit ciphertext.

**Note on engine version in this inventory:** packages and
`.grok/joins/stalwart-current.md` pin **Stalwart 0.16.15** (release binary FOD).
Older docs once said "current (see package after bump)" or mixed
historical 0.11.8 / TOML / :8081. Living modules + packaging join win for version and
ports; docs lag is called out per row where it matters.

---

## 1. Mail engine is Stalwart

- **Claim the tree currently implies:** SMTP, submission, IMAP, ManageSieve,
  JMAP, spam path, and the authoritative mailbox live in **Stalwart**. Surmount
  does not ship a parallel MTA or second mailbox corpus. Alternatives
  (Postfix+Dovecot+Rspamd, Mailcow, custom Rust MTA) are noted as not day-one.
- **Where it lives:** `modules/mail.nix`, `modules/stalwart-service.nix`,
  `docs/open-choices.md` (Mail engine), `docs/STACK.md`, `README.md`,
  `crates/management-ui` (probes Stalwart HTTP only).
- **Why someone might have put it there:** One integrated engine covers
  protocols + store + FTS + admin APIs; faster greenfield than composing
  classic stack pieces; matches operator direction toward a maintainable
  single-VPS mail box.
- **Status:** **required by working code** for mail path (unit, package,
  firewall ports, import helper target Stalwart). Engine *choice* vs other
  MTAs is **proposed in docs**, not a written operator lock.
- **Should revisit now that Stalwart is 0.16.15?** **No** for "keep Stalwart
  as engine" unless the operator wants a different MTA. **Yes** for how we
  configure it (TOML gone; JMAP objects + apply; first-boot defaults).
- **Open question for operator:** Keep Stalwart as the only mail engine for
  production cutover, or should any classic-stack piece still be on the table?

---

## 2. Stalwart version pin: release binary FOD at 0.16.15

- **Claim the tree currently implies:** Engine is Surmount-owned **0.16.15**
  release binary FODs (`stalwart-{arch}-unknown-linux-gnu.tar.gz`), plus FODs
  for cli 1.0.12, webui 1.0.7, spam-filter 3.0.0.
  `packagingMode = "release-binary-fod"`. Host channel is **nixos-26.05**;
  engine is **not** the channel package (channel `pkgs.stalwart` still lags).
  Source `rustPlatform` build was attempted and not shipped (crates.io 403 on
  packaging day 2026-07-30; still deferred).
- **Where it lives:** `nix/packages/stalwart-mail.nix`,
  `stalwart-cli.nix`, `stalwart-webui.nix`, `stalwart-spam-filter.nix`,
  `flake.nix` overlay, `.grok/joins/stalwart-current.md`.
- **Why someone might have put it there:** Greenfield prefers current major.
  Early scaffold briefly pulled channel **0.11.8** while the host was still on
  **nixos-25.05**; that was channel lag, not a product pin. Unstable module
  still TOML / 0.15-shaped when checked; binary FOD is hermetic when source
  vendor fails. Host later moved to **26.05**; engine stayed Surmount FOD.
- **Status:** **required by working code** (eval/build green on these
  packages). Pin approach is **scaffold / packaging choice**, not operator
  "only binaries forever."
- **Should revisit now that Stalwart is 0.16.15?** **Partially done** (we are
  on 0.16.15). **Yes** later for: bump process ownership, source build when
  vendor/rustc allow, whether binary trust model is acceptable long-term.
- **Open question for operator:** Is release-binary FOD acceptable until a
  hermetic source build works, or must Surmount build Stalwart from source
  before MX is live?

---

## 3. Surmount-owned service module; both stock nixpkgs paths disabled

- **Claim the tree currently implies:** Surmount dual-disables both stock
  paths: `services/mail/stalwart-mail.nix` (older / 25.x TOML path) and
  `services/mail/stalwart.nix` (26.05+ stock TOML module). Surmount owns
  `modules/stalwart-service.nix`. Option path is **`services.stalwart`**
  (matches stock attr name). Surmount still owns 0.16 **config.json** (not
  the stock TOML module body). Unit/state stay `stalwart-mail*`. `settings`
  is accepted and **ignored** (no TOML writer). Day-2 config is WebUI or
  `stalwart-cli apply`.
- **Where it lives:** `modules/stalwart-service.nix`, `modules/mail.nix`,
  `modules/default.nix`, join `stalwart-current.md`.
- **Why someone might have put it there:** Stalwart 0.16 dropped TOML; stock
  modules still assume older config and would conflict if left enabled. Option
  name aligns with nixpkgs 26.05; dual-disable keeps Surmount ownership.
- **Status:** **required by working code** for 0.16.15 to run at all on this
  flake.
- **Should revisit now that Stalwart is 0.16.15?** **Yes** for declarative
  apply plans under Nix (listeners, spam URL, TLS, loopback binds). **No** for
  going back to TOML or adopting the stock module body under `services.stalwart`.
- **Open question for operator:** How much first-boot and day-2 Stalwart config
  should be declarative in-repo (`stalwart-cli apply` plans) versus operator
  WebUI clicks?

---

## 4. Store backends: single RocksDB co-location

- **Claim the tree currently implies:** One RocksDB at
  `/var/lib/stalwart-mail/db` holds the data plane (conceptually data, blob,
  FTS, lookup / internal directory). `storeType = "RocksDb"`. No Postgres, S3,
  Redis, ES, Meilisearch, or FoundationDB in modules. Product code must not
  open engine DB files; JMAP/admin/CLI only.
- **Where it lives:** `modules/mail.nix`, `modules/stalwart-service.nix`
  (`config.json`), `modules/options.nix` (`mailDataDir`),
  `docs/DATASTORES.md`, `docs/open-choices.md`, `docs/STACK.md`.
- **Why someone might have put it there:** Stalwart single-node guidance;
  minimize daemons on one VPS; one restic tree; matches historical nixpkgs
  non-legacy default shape (even though our module is Surmount-owned now).
- **Status:** **scaffold default** wired in code; **proposed** with exit gates
  in DATASTORES.md. **Not** operator-accepted architecture. Four-store
  *logical* model is upstream fact + working model; physical co-location is
  provisional. Note: 0.16 expresses store as DataStore JSON; role keys are no
  longer TOML `storage.*` on disk (docs may lag wording).
- **Should revisit now that Stalwart is 0.16.15?** **Yes.** Re-verify
  defaults, backup story, blob/FTS behavior, and any new store knobs against
  0.16 docs / `UPGRADING/v0_16.md`. Historical 0.11.8 store evidence is
  version-pinned research only.
- **Open question for operator:** Keep all-RocksDB co-location through first
  production mail, or pick another physical layout before first durable write?

---

## 5. Single VPS; multi-host deferred

- **Claim the tree currently implies:** One mail VPS runs the stack. Modules
  stay separable for clarity. No multi-node Stalwart, shared FDB/S3, or
  multi-host IdP design in-tree. "Not single-host forever," but no active
  multi-host work.
- **Where it lives:** `docs/open-choices.md`, `docs/STACK.md`,
  `hosts/mail-vps/`, firewall and loopback assumptions in modules.
- **Why someone might have put it there:** Replace one offline Synology;
  operator-deferred scale; avoid premature distributed complexity.
- **Status:** **scaffold default** + **operator-deferred** in docs. Host
  sample is single-node by construction.
- **Should revisit now that Stalwart is 0.16.15?** **No** unless volume,
  compliance, or HA need appears. 0.16 does not by itself force multi-host.
- **Open question for operator:** What concrete trigger (mailbox count, uptime,
  compliance) should end "multi-host deferred"?

---

## 6. Edge: nginx transitional, Caddy target

- **Claim the tree currently implies:** HTTPS today is **nginx** +
  `security.acme` in `modules/web.nix`. Documented **target** is **Caddy**.
  Pure-Rust edge is longer-term research only. Mail ports are **not**
  edge-proxied; they terminate on Stalwart.
- **Where it lives:** `modules/web.nix`, `docs/EDGE_AND_TLS.md`,
  `docs/open-choices.md`, `docs/STACK.md`, `README.md`.
- **Why someone might have put it there:** nginx was fastest mature NixOS path
  to ACME + reverse proxy; operator preference to leave nginx later; Caddy
  simpler ACME mental model for few vhosts.
- **Status:** nginx is **required by working code** today. Caddy is
  **proposed in docs** only (not implemented).
- **Should revisit now that Stalwart is 0.16.15?** **Indirectly yes:** 0.16
  first-boot may bind HTTPS :443 and HTTP :8080 itself, which collides with
  "edge owns 443" and "UI on loopback." Edge cutover and Stalwart listener
  rebind should be planned together.
- **Open question for operator:** Cut over to Caddy before production MX, or
  stay on nginx through first live mail and migrate later?

---

## 7. No required Cloudflare hop

- **Claim the tree currently implies:** Mail and core HTTPS work with DNS
  **A/AAAA straight to the VPS**. No required orange-cloud proxy, CF Access,
  Workers, WAF, or Tunnel. Registrar may still be Cloudflare DNS-only.
- **Where it lives:** `docs/open-choices.md`, `docs/EDGE_AND_TLS.md`,
  `docs/SECURITY.md`, `docs/STACK.md`, `docs/hygiene.md`, `README.md`.
- **Why someone might have put it there:** Own origin TLS and mail reputation;
  avoid CDN-shaped failure modes on SMTP; keep the stack rebuildable without
  a third-party proxy account.
- **Status:** **proposed in docs** and reflected in module design (firewall
  opens origin ports; no CF modules). Not a runtime dependency in code.
- **Should revisit now that Stalwart is 0.16.15?** **No** for the CF rule
  itself. Engine version is unrelated.
- **Open question for operator:** Confirm no-required-CF for production, or
  is any CF product (Access, Tunnel, WAF) intentionally in-scope later?

---

## 8. Product web stack: Axum + Leptos SSR; admin then webmail

- **Claim the tree currently implies:** HTTP shell is **Axum**. Invested UI
  path is **Leptos SSR**. Admin `GET /` is **Leptos SSR scaffold** (ssr-only;
  `leptos` in Cargo.toml; marker `data-surmount-ssr="leptos"`). Hydrate /
  richer admin / webmail residual. Phase order: **admin first**, then **real
  webmail in v1** via JMAP. Stalwart `/admin` is bootstrap fallback.
  Desktop/local-first clients are complementary later, not a cancel of
  browser admin/webmail. No primary React/Vue SPA.
- **Where it lives:** `crates/management-ui/` (`Cargo.toml`, routes),
  `modules/management-ui.nix`, `nix/packages/management-ui.nix`,
  `docs/SEARCH_AND_UI.md`, `docs/open-choices.md`, `docs/STACK.md`.
- **Why someone might have put it there:** Rust-aligned product layer; SSR for
  admin/webmail without a separate SPA toolchain; skeleton speed with embedded
  HTML; ops before end-user mail UX.
- **Status:** Axum edge + Leptos SSR home shell are **required by working
  code** (ssr-only scaffold). Hydrate islands, richer admin pages, and
  webmail phase order remain **residual / proposed**. JMAP proxy route
  returns **501**.
- **Should revisit now that Stalwart is 0.16.15?** **Yes** for API surface:
  management/JMAP URLs, auth to 0.16 HTTP on :8080, schema-driven CLI vs old
  import flags. Framework choice (Axum/Leptos) is independent of engine patch.
- **Open question for operator:** Is admin-then-webmail-in-v1 still the phase
  order you want, and is Leptos still the UI investment (vs stay thinner longer)?

---

## 9. Nostr-first product auth

- **Claim the tree currently implies:** Operators (later users) authenticate to
  **Surmount** with **npub + NIP-98** (and sessions). Stalwart keeps ordinary
  mail credentials (passwords / app passwords). Surmount bridges identity to
  Stalwart APIs. Server never stores **nsec**. Stalwart is not claimed to
  speak Nostr. OIDC/LDAP bridge is optional later, not v1 requirement.
- **Where it lives:** `docs/open-choices.md`, `docs/SECURITY.md`,
  `docs/SEARCH_AND_UI.md`, `docs/STACK.md`, `docs/SECRETS.md`. **Not**
  implemented in `crates/management-ui` yet (no Nostr deps or auth routes).
- **Why someone might have put it there:** Product identity separate from mail
  directory; crypto auth without password reuse on the admin UI; fits Surmount
  product direction.
- **Status:** **proposed in docs only.** Working UI has no auth gate.
- **Should revisit now that Stalwart is 0.16.15?** **No** for Nostr-vs-password
  at the product edge. **Yes** for how the bridge issues/rotates Stalwart
  tokens under 0.16 directory/API changes.
- **Open question for operator:** Proceed with Nostr-first for operator login,
  and what is the first-operator bootstrap allowlist / key-loss story?

---

## 10. Secrets: sops-nix / Vaultwarden / LUKS (three layers)

- **Claim the tree originally implied (superseded on git storage):**

  | Layer | Tool | Job |
  |-------|------|-----|
  | A Deploy | **sops-nix + age** | Decrypt on host at activation |
  | B Humans | **Vaultwarden** (planned) | Passwords, TOTP, notes |
  | C Disk | **LUKS2** when install allows | Offline disk / snapshot |

  sops-nix is primary; **no dual-stack agenix**. Vaultwarden does not replace
  sops or encrypt mail RocksDB. Flake pure eval must not need a live vault.
- **Hygiene correction 2026-07-30:** deploy and LUKS unlock material are
  **never** in public git (plain or ciphertext). Host/out-of-band only.
  Living law: [../hygiene.md](../hygiene.md), [../SECRETS.md](../SECRETS.md).
- **Where it lives:** `modules/secrets.nix`, `docs/SECRETS.md`,
  `docs/SECURITY.md`, `docs/open-choices.md`, `secrets/README.md`,
  host TODOs in `hosts/mail-vps/configuration.nix`.
- **Why someone might have put it there:** Separate jobs that people mash;
  hermetic flake; self-hosted human vault; FDE when provider/install allows;
  common upstream sops-in-git pattern (rejected here for public repo).
- **Status:** Layer A **wired as defaults** (no real host secrets yet =
  scaffold). Layer B **proposed**, not a module. Layer C **proposed
  posture**, sample host is unencrypted ext4 label root. Git storage of
  secrets: **forbidden**.
- **Should revisit now that Stalwart is 0.16.15?** **Yes** for how admin
  bootstrap secrets map into 0.16 (`STALWART_RECOVERY_ADMIN`, credentials
  macros, apply-time secrets). Layer split itself is independent of version.
- **Open question for operator:** Approve the three-layer split as production
  posture, and when should Vaultwarden and LUKS install path become real work?

---

## 11. Static legacy sites only

- **Claim the tree currently implies:** Old Synology web content is **static
  files** at the edge (`extraVhosts` / future `staticSites`). No PHP/Node app
  servers for legacy sites. Apex/www currently redirect to services host as a
  park. New product apps are separate services.
- **Where it lives:** `modules/web.nix`, `modules/options.nix`
  (`web.extraVhosts`), `docs/open-choices.md`, `docs/STACK.md`,
  `docs/SECURITY.md`, `README.md`.
- **Why someone might have put it there:** Reduce attack surface; MailPlus
  replacement focus; old sites often static or exportable as files.
- **Status:** **proposed / scaffold default** in modules and docs. No staged
  content roots in sample host yet.
- **Should revisit now that Stalwart is 0.16.15?** **No.**
- **Open question for operator:** Which legacy hostnames and document roots
  must be live at cutover, and is "static only" still true for all of them?

---

## 12. Fix / FixOS ladder level (L0 + early L1)

- **Claim the tree currently implies:** Working names **Fix** (Nix lineage)
  and **FixOS** (NixOS lineage), etymology Facta Non Verba. Honest ladder L0
  through L4. **Today:** mostly **L0** (consume upstream nixpkgs) with **early
  L1** (Surmount packages/modules: management-ui crane build, Stalwart FODs,
  `modules/*`). This repo is a FixOS **consumer/seed**, not the whole Fix
  monorepo. L2 soft channel and L3/L4 forks are **not started**.
- **Where it lives:** `docs/fix-and-fixos.md`,
  `docs/research/fix-fixos-ladder.md`, `docs/open-choices.md`, `AGENTS.md`,
  `.grok/joins/fix-fixos.md`.
- **Why someone might have put it there:** Name ownership path for current
  engines and Surmount defaults without pretending a full Nix/NixOS fork on
  day one.
- **Status:** **open working direction / research naming.** Not
  operator-accepted product law. Packaging work is de facto early L1 evidence.
- **Should revisit now that Stalwart is 0.16.15?** **Yes** as evidence that L1
  "owned hot package" is already real for mail. Does not auto-climb to L2+.
- **Open question for operator:** Stay at L0+L1 inside `surmount-server` for
  now, or split packaging / soft channel (L2) while greenfield is cheap?

---

## 13. Management UI port 8090 vs Stalwart HTTP 8080

- **Claim the tree currently implies:** Management UI listens
  **127.0.0.1:8090** (default). Stalwart 0.16 first-boot HTTP management is
  **:8080** (upstream safe defaults; may bind all interfaces). nginx proxies
  `/` -> UI :8090 and `/stalwart-admin/` -> `127.0.0.1:8080`. UI env
  `SURMOUNT_STALWART_URL=http://127.0.0.1:8080`. Living OPS/MIGRATION/
  SEARCH_AND_UI mopped to that story (2026-08-07). Some joins may still lag.
- **Where it lives:** `modules/options.nix` (port 8090),
  `modules/management-ui.nix`, `modules/web.nix`, `modules/mail.nix`,
  `modules/networking.nix` (comments), `.grok/joins/stalwart-current.md`.
  Historical lag examples included old MIGRATION :8081; residual join risk:
  `.grok/joins/foundation.md`.
- **Why someone might have put it there:** Avoid port fight after 0.16
  first-boot took :8080; keep bootstrap admin path; loopback-only product UI.
- **Status:** **required by working code** for current defaults. Public
  exposure of Stalwart :8080 is a **known gap** (rebind to loopback before
  production).
- **Should revisit now that Stalwart is 0.16.15?** **Yes** (this *is* a 0.16
  consequence). Confirm production binds, firewall, and doc cleanup to one
  port story.
- **Open question for operator:** Keep UI 8090 + Stalwart 8080 (loopback), or
  prefer a different permanent split (e.g. Stalwart HTTP only on a private
  port you choose via apply)?

---

## 14. Import / migration: Maildir path, operator-run only

- **Claim the tree currently implies:** Synology MailPlus migration uses
  **nested Maildir** as source of truth (not MailPlus SQLite). Staging path
  **`/var/lib/surmount/import/maildir`**. Helper
  `surmount-mail-import-maildir` calls `stalwart-cli import messages
  --format maildir-nested`. **Never** auto-import in activation scripts.
  Import staging is disposable after verified import; live authority is
  Stalwart store only.
- **Where it lives:** `modules/mail.nix` (helper + tmpfiles),
  `docs/MIGRATION.md`, `docs/DATASTORES.md`, `docs/open-choices.md`,
  `README.md`.
- **Why someone might have put it there:** Lossless content recovery; operator
  control; avoid silent data mutation on every rebuild.
- **Status:** Helper and staging dir are **in working code** (template).
  Exact 0.16 CLI import subcommands are **unknown / needs confirm** per
  packaging join (schema-driven CLI may differ from older `import messages`).
- **Should revisit now that Stalwart is 0.16.15?** **Yes.** Validate real
  import path against `stalwart-cli --help` and 0.16 docs before any
  production MailPlus cutover. MIGRATION.md ports mopped to **8080** (2026-08-07).
- **Open question for operator:** Is Maildir-nested still the only import
  path you need at cutover, and who runs the verified trial import?

---

## 15. Backups: restic, opt-in

- **Claim the tree currently implies:** Backups use **restic** via
  `modules/backups.nix`, **disabled** until `repository` and `passwordFile`
  are set. Default paths: `mailDataDir` + `stateDir` (+ optional extras).
  Daily timer, conservative prune. Password from sops. RocksDB consistency:
  prefer stop-service or FS snapshot (documented, not automated freeze).
  Offline copies of age / LUKS / restic password are separate from the repo.
- **Where it lives:** `modules/backups.nix`, `modules/options.nix`,
  `docs/OPS.md`, `docs/DATASTORES.md` sections 7-8, `docs/SECRETS.md`,
  host sample (commented).
- **Why someone might have put it there:** Encrypted off-box backups without
  forcing a provider choice in scaffold; operator enables when ready.
- **Status:** **scaffold only** (module present, not enabled on sample host).
  RPO/RTO **unset**.
- **Should revisit now that Stalwart is 0.16.15?** **Yes** for consistency
  procedure against 0.16 RocksDB layout; not for restic-as-tool choice unless
  operator prefers something else.
- **Open question for operator:** Where should the restic repository live, and
  what RPO/RTO do you want before MX is live?

---

## 16. Spam-filter (and WebUI) FODs not auto-applied

- **Claim the tree currently implies:** spam-filter **3.0.0** and webui
  **1.0.7** are fetched as FODs and installed under
  `/etc/surmount/stalwart/` (`spam-filter.toml`,
  `spam-filter-rules.json.gz`, `webui.zip`). They are **not** automatically
  pointed into the running engine. 0.16 first-boot may still try GitHub for
  resources until operator WebUI or `stalwart-cli apply` sets hermetic
  `file://` URLs. Spam-filter TOML is **anti-spam config**, not the message DB.
- **Where it lives:** `nix/packages/stalwart-spam-filter.nix`,
  `stalwart-webui.nix`, `modules/mail.nix` (`environment.etc`),
  `.grok/joins/stalwart-current.md`, `docs/DATASTORES.md` section 5.4
  (version numbers in older prose may lag).
- **Why someone might have put it there:** Hermetic builds (no impure empty
  nixpkgs spam path); avoid baking wrong runtime wiring before 0.16 apply
  schema is understood.
- **Status:** FOD install is **required by working code**. Runtime wiring is
  **known gap / scaffold**.
- **Should revisit now that Stalwart is 0.16.15?** **Yes.** This gap is
  specifically a 0.16 config-model issue. Prefer a committed apply plan.
- **Open question for operator:** Require hermetic spam/webui on first boot
  (no GitHub fetch), or accept one-time operator apply after install?

---

## 17. Domains, hostnames, and sample accounts

- **Claim the tree currently implies:** Primary domain **`surmount.systems`**.
  Mail host **`mail.surmount.systems`**. Services UI
  **`services.surmount.systems`**. ACME email **`admin@surmount.systems`**.
  Sample declarative account *names* `admin` and `postmaster` (passwords via
  sops or out-of-band). Additional domains list empty by default.
- **Where it lives:** `modules/options.nix` defaults,
  `hosts/mail-vps/configuration.nix`, `docs/DNS.md`, `README.md`.
- **Why someone might have put it there:** Real production names as scaffold
  defaults so DNS/docs match one story.
- **Status:** **scaffold defaults** in options + sample host. DNS not proven
  by this inventory.
- **Should revisit now that Stalwart is 0.16.15?** **No** for names.
  **Yes** for how domains/accounts get into 0.16 directory (apply vs WebUI vs
  declarative Nix that does not yet create principals).
- **Open question for operator:** Are these hostnames and bootstrap local-parts
  final for cutover?

---

## 18. Network perimeter and hardening sketch

- **Claim the tree currently implies:** Firewall allows 22, 25, 80, 443, 465,
  587, 993, 4190. Does **not** intentionally publish management UI or Stalwart
  HTTP (but 0.16 default may bind :8080/:443 broadly until rebind). SSH keys
  preferred (`allowPasswordAuth` default false). fail2ban sshd jail on by
  default. Mail abuse primarily Stalwart spam/greylist, not a third-party WAF.
- **Where it lives:** `modules/networking.nix`, `modules/hardening.nix`,
  `docs/OPS.md`, `docs/SECURITY.md`.
- **Why someone might have put it there:** Minimal public surface; boring
  defaults; self-ops without CF WAF.
- **Status:** **scaffold defaults** in working modules. Not a full hardened
  baseline or pen-test claim.
- **Should revisit now that Stalwart is 0.16.15?** **Yes** for listener binds
  (HTTP/HTTPS/POP3 defaults) vs firewall story.
- **Open question for operator:** Lock SSH to admin nets before go-live, and
  confirm which mail-adjacent ports (e.g. POP3 995) you want open at all?

---

## 19. Stalwart first-boot listener defaults (0.16)

- **Claim the tree currently implies:** Empty RocksDB first boot runs upstream
  `insert_safe_defaults`: SMTP :25, submissions :465, IMAPS :993, ManageSieve
  :4190, HTTP **:8080**, HTTPS **:443**, POP3S **:995**, often on `[::]:port`.
  Surmount does **not** declaratively pin listeners in Nix today.
- **Where it lives:** `modules/mail.nix` headers, `modules/stalwart-service.nix`,
  `.grok/joins/stalwart-current.md`, comments in `networking.nix` / `web.nix`.
- **Why someone might have put it there:** 0.16 moved listeners into datastore;
  scaffold ships store path only until apply plans exist.
- **Status:** **upstream default behavior** the tree accepts for now. **Known
  production gap** (443 conflict with nginx, public :8080, POP3 maybe unwanted).
- **Should revisit now that Stalwart is 0.16.15?** **Yes. High priority**
  before public MX/HTTPS.
- **Open question for operator:** Which listeners must be public on day one,
  and should HTTPS for mail TLS stay on Stalwart files from `security.acme`
  rather than Stalwart-owned :443?

---

## 20. Search ownership: Stalwart FTS first

- **Claim the tree currently implies:** Mail search uses Stalwart native FTS +
  JMAP `Email/query`. No parallel Surmount full-text index of mail bodies.
  External ES/Meilisearch only if measurement fails gates. Surmount-side
  indexes only for non-mail product data (audit, ops) with a named need.
- **Where it lives:** `docs/SEARCH_AND_UI.md`, `docs/open-choices.md`,
  `docs/DATASTORES.md`, `docs/STACK.md`.
- **Why someone might have put it there:** Avoid dual corpus; engine already
  indexes; product UI should not reimplement mail search.
- **Status:** **proposed in docs.** No product search implementation yet.
  Internal FTS quality **unmeasured**.
- **Should revisit now that Stalwart is 0.16.15?** **Yes** for FTS defaults and
  JMAP query behavior on current engine; keep "no parallel mail FTS" unless
  evidence says otherwise.
- **Open question for operator:** Any mail search quality bar that would force
  external FTS before v1 webmail?

---

## 21. Product / UI durable state under `/var/lib/surmount`

- **Claim the tree currently implies:** Surmount state root
  **`/var/lib/surmount`** for import staging and future sessions / npub map /
  audit. Prefer small dedicated SQLite or files there, **not** stuffing product
  state into Stalwart RocksDB. Management UI skeleton needs little durable
  state today; service `ReadWritePaths` includes `stateDir`.
- **Where it lives:** `modules/options.nix` (`stateDir`),
  `modules/management-ui.nix`, `docs/DATASTORES.md` sections 7.2 / inventory.
- **Why someone might have put it there:** Clear ownership boundary; backup
  path shared with restic `stateDir`.
- **Status:** **scaffold default** path exists; product DB **not implemented**.
- **Should revisit now that Stalwart is 0.16.15?** **No** for the boundary.
  **Yes** when Nostr sessions land (cookie vs server session store shape).
- **Open question for operator:** Session store preference (signed cookie vs
  server-side) when Nostr auth lands?

---

## 22. Hermetic flake; nixpkgs 26.05 host channel; crane UI

- **Claim the tree currently implies:** Locked `flake.lock`; hermetic packages
  under `nix/packages/` (crane for management-ui; FODs for Stalwart bits);
  `ref/` study-only; host OS from **nixos-26.05** while engines can be newer
  via overlays. Sample host `system.stateVersion = "26.05"`. Crane wants
  nixpkgs >= 26.05 (no more 25.05 evaluation warning).
- **Where it lives:** `flake.nix`, `flake.lock`, `nix/packages/*`,
  `nix/rust-toolchain.nix`, `docs/hygiene.md`, `hosts/mail-vps/configuration.nix`.
- **Why someone might have put it there:** Reproducible mail host; pure CI;
  stable OS channel while overriding hot packages.
- **Status:** **living** (operator direction 2026-08-07: use 26.05).
- **Open question for operator:** Further channel bumps only with release notes
  + measured need.

---

## 23. Hosting preference: operator-chosen VPS, PTR control, LUKS install path

- **Claim the tree currently implies:** **Operator-chosen VPS**;
  operator-controlled **PTR/rDNS**; install path that can do **LUKS2** via
  disko + nixos-anywhere when possible. Sample host uses qemu-guest profile
  and labeled ext4 root (placeholder). **Size/plan open (Q-HOST-1)**; public
  docs must not invent provider names or RAM/disk/core/SKU numbers.
- **Where it lives:** `README.md` (Hosting), `docs/SECURITY.md`,
  `docs/operator-direction.md`, `hosts/mail-vps/configuration.nix`.
- **Why someone might have put it there:** Mail-friendly VPS + rDNS; FDE when
  greenfield install allows.
- **Status:** **operator direction** for operator-chosen host; size remains
  open. Invented brand/SKU hosting copy scrubbed from living docs (2026-08-07).
- **Open question for operator:** Which provider and plan is production
  (**Q-HOST-1**), and will first install be LUKS or interim plain disk
  (**Q-HOST-2**)?

---

## 24. Self-ops: journald, scripts, health endpoints

- **Claim the tree currently implies:** Operate with **journald**, UI
  `/health` and `/api/v1/stalwart/status`, bash scripts under `scripts/` for
  DNS/TLS/mail port checks, runbooks in `docs/`. No hard dependency on a SaaS
  log sink or third-party WAF. Future flake apps for checks.
- **Where it lives:** `docs/OPS.md`, `scripts/*`, `crates/management-ui`,
  `docs/open-choices.md`.
- **Why someone might have put it there:** Single-operator maintainability;
  boring local tools.
- **Status:** **proposed + partial scaffold** (scripts and health exist;
  structured UI logs and aggregate readiness are future).
- **Should revisit now that Stalwart is 0.16.15?** **Mild yes** (tracer/log
  settings and CLI diagnostics differ from 0.11). Tooling posture unchanged.
- **Open question for operator:** Any required external monitoring (Uptime,
  Prometheus, etc.) before go-live, or journald + scripts enough at first?

---

## 25. ACME certs for web; Stalwart mail TLS still operator TODO

- **Claim the tree currently implies:** Let's Encrypt via `security.acme` for
  nginx vhosts (`services`, `mail`, apex, www). Mail hostname gets a cert
  useful for **later** Stalwart IMAPS/SMTPS file paths. Wiring cert paths into
  Stalwart is **not done** (host TODO; day-2 apply/WebUI in 0.16 world).
- **Where it lives:** `modules/web.nix`, `hosts/mail-vps/configuration.nix`,
  `docs/EDGE_AND_TLS.md`, `docs/DNS.md`.
- **Why someone might have put it there:** Web HTTPS first; mail TLS depends on
  DNS + engine config model.
- **Status:** ACME for nginx is **in working code**. Stalwart TLS file use is
  **open / TODO**.
- **Should revisit now that Stalwart is 0.16.15?** **Yes.** Cert objects are
  datastore/apply territory; also conflict risk if Stalwart defaults bind :443.
- **Open question for operator:** Should mail protocol TLS use the same
  `security.acme` material as nginx, and who owns renewal on the Stalwart side?

---

## 26. Vaultwarden data engine (when planned)

- **Claim the tree currently implies:** When Vaultwarden is added, **SQLite**
  is the recommended default on a single VPS; Postgres only if already running
  PG or multi-instance VW. Private hostname, no public signup, admin token from
  sops, restic includes VW data dir.
- **Where it lives:** `docs/SECRETS.md`, `docs/DATASTORES.md` section 7.1,
  comments in `modules/secrets.nix`. **No** `services.vaultwarden` module yet.
- **Why someone might have put it there:** Minimize daemons; simple backup;
  layer-B story documented before code.
- **Status:** **proposed in docs only.**
- **Should revisit now that Stalwart is 0.16.15?** **No** (independent
  service). Shared Postgres with mail metadata remains an open DATASTORES
  question if mail ever leaves RocksDB.
- **Open question for operator:** When should Vaultwarden land relative to MX
  cutover (before, with, or after)?

---

## 27. Doc and join lag (meta-assumption risk)

- **Claim (living modules):** Stalwart HTTP management **:8080**, Surmount UI
  default **:8090**, engine **0.16.15** config.json via `services.stalwart`,
  spam-filter **3.0.0** FODs under `/etc/surmount/stalwart/`, stock modules
  dual-disabled.
- **Where lag lived:** older STACK/MIGRATION/OPS/SEARCH port rows, DATASTORES
  5.2 TOML eval, foundation join. Mop pass 2026-08-07 fixed living MIGRATION /
  OPS / SEARCH_AND_UI ports, DATASTORES 5.2, and historical option past-tense.
- **Remaining risk:** `.grok/joins/foundation.md` and any un-mopped diagram
  cells may still lag; trust `modules/*` + COMPACTION-PIN +
  `.grok/joins/stalwart-current.md` over old joins.
- **Status:** major living-doc debt mopped; joins may still lag.
- **Should revisit now that Stalwart is 0.16.15?** Living product docs should
  stay aligned on each packaging/port change (not a separate forever backlog).

---

## Coverage checklist (requested topics)

| Topic | Section |
|-------|---------|
| Mail engine Stalwart + binary FOD pin | 1, 2, 3 |
| Store backends / RocksDB co-location | 4 |
| Single VPS vs multi-host | 5 |
| Edge nginx transitional, Caddy target | 6 |
| No Cloudflare | 7 |
| Axum + Leptos, admin then webmail | 8 |
| Nostr auth | 9 |
| sops / Vaultwarden / LUKS | 10, 26 |
| Static legacy sites | 11 |
| Fix/FixOS ladder level | 12 |
| Management UI 8090 vs Stalwart 8080 | 13 |
| Import/migration Maildir path | 14 |
| Secrets, backups restic | 10, 15 |
| Spam-filter FOD not auto-applied | 16 |
| Other material | 17-25, 27 |

---

## Suggested operator pass (optional reading order)

1. Engine + pin approach (1-3) and listener/port reality (13, 19).
2. Store + backup before first durable mail (4, 15).
3. Edge/CF/TLS (6, 7, 25).
4. Auth and UI phases (8, 9).
5. Secrets layers and hosting/LUKS (10, 23).
6. Fix ladder only if packaging ownership is the next investment (12).

When the operator accepts or rejects a row, update
[../open-choices.md](../open-choices.md) and the owning living doc in the
same turn. Do not treat this research file as acceptance.
