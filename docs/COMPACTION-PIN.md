# Compaction pin (reload after context loss)

**Single reload file** for humans and agents after chat compaction.
Read this first, then follow links into child docs. Do not invent product
scope from memory alone.

**Last updated:** 2026-09-02 (before asking the operator to deploy or
switch, agents run local Nix eval that does not rustc on this laptop,
then `just deploy-host -- --dry-run`, and report those results; the
real switch stays operator-owned unless they override). Prior 2026-08-27 (Axum TLS key is **0600** owner-only `surmount-ui`;
Stalwart mail-plane TLS uses copies under `secrets/mail/tls`. Living mailbox
map stays in operator-facts; operator bins are `nix run .#...`.) Prior 2026-08-25 (living mailbox map stays in operator-facts; operator bins are `nix run .#...`.) Prior 2026-08-22 (living mailbox map:
`~/.agents/surmount-server/operator-facts.md`. Prior 2026-08-20 (mail domains get primary-class DNSSEC, not
leftover-unsigned. Leftover unmatched parent DS is sequencing: clear
then sign with Namecheap hosted **DNSSEC Status ON**, same as
`surmount.systems`. ECDSA P-256 SHA-256 algorithm 13 acceptable for now.
**Live measure 2026-08-20:** `surmount.systems` operator already clicked
ON; public still waiting for DS+DNSKEY (Insecure, not SERVFAIL).
`cryptoquick.com` leftover parent DS key tag **2368** alg 13 digest type
1 SHA-1 **still present**; no apex DNSKEY; validating SERVFAIL. Operator
click **ON** now. Do **not** re-add 2368. SERVFAIL can persist until
stale DS drops. Prove Secure after wait. HTTPS names on the leaf for
cryptoquick **after** validating lookups succeed, not this turn while
SERVFAIL. `list` surfaces EmailType; `--live set-mx` fails closed while
EmailType is FWD; that is not a public MX flip while Email Forwarding
is on. Extra static vhost HTTPS **live** for six Namecheap zones
apex+www; production leaf is 17 certificate hostnames). Prior same
day: operator asked primary MX to `mail.surmount.systems`; laptop
`list` worked; FWD still published eforward. Prior same day: dual onion
discovery on every public HTTP
Host this edge serves, including extra static vhosts and MTA-STS;
`/_o/{host}` onion routing; mail unmapped. Prior same day: mail records
on every mailbox domain, not primary-only; leftover parent DS is a
SERVFAIL class to clear then sign. Prior same day: Administrator attach
npub on `/mail` Grant console login; session CSRF; map write without
directory listing.
Prior 2026-08-19 (apex/www serve packaged SurmountSystems/site
tip `1c84696`; COMING SOON leftover closed; services console unchanged).
Prior 2026-08-18 (first packaged GitHub site on apex/www). Prior 2026-08-17
(Onion-Location + Alt-Svc on Axum edge; live Arti unit active; same v3
for apex/www/services; Tor Browser verify and operator HS backup still
residual; B3 not fully closed). Prior 2026-08-12
(earn-trust live Tracks 0/A/B/C: UNDER CONSTRUCTION, dual DKIM register,
mta-sts on LE leaf, MTA-STS testing live, durable recovery.env; B4 Nostr;
Q-AUTH-1 open)
**Status:** living direction pin (operator direction + tree facts). Not every
line is eternal law. Prefer **operator direction YYYY-MM-DD**, **proposed**,
**scaffold default**, **research finding**, or **open** labels. Do not say
"we decided" or "locked" without explicit operator approval of that item.

**Companions (always):**
[operator-direction.md](operator-direction.md),
[hygiene.md](hygiene.md),
[principles.md](principles.md),
[STACK.md](STACK.md),
[open-choices.md](open-choices.md),
[../AGENTS.md](../AGENTS.md).

---

## 1. What this project is

**Surmount Server** is a hermetic **NixOS** mail + web stack that replaces an
offline **Synology DiskStation (MailPlus + static sites)** with a rebuildable
flake-owned host.

| Piece | Role |
|-------|------|
| Primary domain | `surmount.systems` |
| Management UI host | `services.surmount.systems` |
| Mail host / MX | `mail.surmount.systems` |
| Form factor | **Single operator-chosen VPS** until the operator says otherwise |
| Public awareness | Treat the tree as **public-domain / public**. Zero secrets in git. |

### Naming (Facta Non Verba)

| Name | Meaning |
|------|---------|
| **Facta Non Verba** | "Deeds, not words." Cultural root of Fix names. |
| **Fix** | Surmount take / fork lineage of **upstream Nix** |
| **FixOS** | Surmount take / fork lineage of **upstream NixOS** |
| **upstream nixpkgs** | Community package set (flake input) |
| **Surmount package overlay** | Packages we own in *this* repo (`nix/packages/`) |
| **future fixpkgs channel** | Later shared Surmount package set (not shipping yet) |

Ladder detail: [fix-and-fixos.md](fix-and-fixos.md).
**FOD** = Nix **Fixed-Output Derivation** only ([glossary.md](glossary.md)).

---

## 2. Absolute hygiene (do not soften)

| Rule | Detail |
|------|--------|
| **NEVER secrets in git** | No plaintext, no ciphertext of production secrets, no age/sops private keys, no LUKS unlock material, no `.env`. **Never suggest** "encrypted in git." Host / operator channels only. [hygiene.md](hygiene.md), [SECRETS.md](SECRETS.md). |
| **Stack language** | **NixOS + Nix + Rust**. **No Python** product/ops (fail2ban transitional-at-most). **No NPM**. Gaps filled in-house. [principles.md](principles.md) section 5b. |
| **No nginx product edge** | In-tree nginx is **transitional-to-delete**. Target **first-party Axum** HTTPS edge. [EDGE_AND_TLS.md](EDGE_AND_TLS.md). |
| **No Cloudflare products** | Direct-to-VPS path required. CF Research blog posts may be **cited for learning** only. No orange-cloud, Workers, Tunnel, CF WAF, CF Access as critical path. |
| **Agents never touch Git's index** | The index is how the operator tracks agent work. No `git add`, `git commit`, `git push`, stage, `git restore --staged`, `git reset`, `git rm --cached`. Do not "clean up" the index. Operator 2026-08-28: stop touching Git. Dual-pin host `~/.grok/AGENTS.md`. |
| **Leftovers: complete sentences, none already done** | Operator 2026-08-28: every numbered leftover is a complete sentence. Do not re-list work the screenshot or live host already shows is done. Dual-pin host `~/.grok/AGENTS.md`. |
| **Prove the operator gate** | Operator 2026-09-02: do not list a step as operator residual without evidence the agent cannot do it (secrets, standing forbid of the real `just deploy-host` switch and of `just check-remote`, hypervisor, Namecheap click). Host-local enable is agent work. Using the TUI is the product, not leftover. Dual-pin `AGENTS.md`. |
| **Eval then deploy dry-run before asking to switch** | Operator 2026-09-02: before any sentence that asks the operator to deploy or switch, agents must have run (a) local Nix eval that does **not** rustc on this laptop (`tests/module-eval.nix` / named flake eval) and (b) `just deploy-host -- --dry-run`, and must report those results. Narrows 2026-08-25: dry-run is required agent work; the real switch stays operator-owned unless they override. Laptop cargo and `BUILD_LOCAL` stay forbidden. Dual-pin `AGENTS.md`. |
| **"Always remember" = dual pin** | Operator 2026-08-27: when they say **always remember**, write it in **AGENTS.md and this table in the same turn**. Chat-only does not survive compaction. One file does not survive attention dilution. |
| **Finish check-remote then dry-run then switch** | Operator 2026-08-27: after a guest reboot or RAM scare, do **not** tell the operator to wait. Leftover stays `just check-remote`, then `just deploy-host -- --dry-run`, then the real switch unless they said stop. Agents never reboot. Agents run eval plus dry-run (2026-09-02). The real switch and `just check-remote` stay operator-owned unless they override. |
| **Forks** | Agents never touch `SurmountSystems/*` fork git unless the operator **explicitly** orders that work. Stalwart fork consume: flake input. PQConnect Nix work on sibling repo stays **unstaged for human** commit. |
| **Always validate versions** | Re-check upstream latest when touching pins. Docs lag. [research/version-audit.md](research/version-audit.md). |
| **Prove assumptions** | No "unsafe" / scare copy without evidence (failed build, broken import, measured issue). |
| **Question ids** | Unique global **Q-*** (or per-file Q1, Q2...). Never collide with section numbers. |
| **Hierarchical subagents** | Multi-file research/impl: coordinator thin; workers own greps/edits; write short **reports** under `~/.agents/reports/` (host home; never recreate repo `.agents/reports` or `.grok/joins` as a live home). |
| **Additive "also"** | New ask stacks; do **not** kill healthy in-flight agents on the prior goal. |
| **No ADR jargon** | Plain American English: open choices, proposed, scaffold default, research finding. |
| **ASCII docs** | No em dashes in docs we own. |
| **No assumed acceptance** | Scaffold and research are not operator lock-in. |

---

## 3. Versions (last audit 2026-07-31; re-verify before bump)

Source of truth for FODs: `nix/packages/*.nix`. Audit:
[research/version-audit.md](research/version-audit.md).

| Pin | Tree (as of audit) | Notes |
|-----|--------------------|-------|
| Stalwart server | **0.16.15** binary FOD | At latest upstream tag (re-check 2026-07-31) |
| stalwart-cli | **1.0.12** | At latest; no `import` subcommand |
| vandelay | **1.0.7** | Official 0.16 Maildir++ / JMAP import |
| Stalwart WebUI | **1.0.7** | At latest |
| spam-filter rules | **3.0.0** | At latest |
| Arti (HS package) | Surmount-owned source **2.5.1** (`arti-onion-service.nix`, GitLab `arti-v2.5.1`) + `onion-service-service` | Matches nixpkgs-rust `arti` 2.5.1 cargoDeps. crates.io max_stable **2.5.1** (re-check 2026-08-27; no 2.6). Not stock nixpkgs 1.4.2 lag; rustc via `nixpkgs-rust` (MSRV 1.91+). Live Tor verify still residual |
| management-ui rustc (crane) | **1.95.0** via `rustPackages_1_95` | Host channel nixos-26.05; Leptos MSRV is >=1.88 |
| Host nixpkgs channel | **nixos-26.05** lock | Engine is **not** from channel; channel lag is separate |
| Stock Stalwart modules | dual `disabledModules` | `stalwart-mail.nix` + `stalwart.nix`; Surmount option `services.stalwart` (not stock TOML body) |
| PQConnect upstream | **1.2.x** Python (tree saw **1.2.3**) | Sibling path packaging; not Rust |
| PQConnect Nix flake | Sibling `feat/nix-flake`, **uncommitted** (human owns commit) | [research/pqconnect-local-packaging.md](research/pqconnect-local-packaging.md) |
| SurmountSystems/stalwart | Fork for patches; flake input pattern | [packages-and-forks.md](packages-and-forks.md) |
| SurmountSystems/pqconnect | Fork / packaging mirror | Path input after human commit |
| Binary FOD RocksDB | Embedded in Stalwart release binary | System `pkgs.rocksdb` is **design goal** on source build |

Compatibility with old Stalwart majors is **not** a goal. Prefer current majors.
nixpkgs lag is not a reason to stay old on engines we own.

---

## 4. Host

| Item | Direction |
|------|-----------|
| Size / plan | **Open (Q-HOST-1).** Do not invent or publish RAM, disk, core counts, or provider SKUs as product law. |
| Provider | **Operator-chosen VPS**. **Do not invent a provider name.** Assume NixOS allowed; operator confirms NixOS + LUKS2 with provider. |
| Topology | **Single VPS** until operator says otherwise. No invented scale-out triggers. |
| Disk | **LUKS2** first-class. Unlock: passphrase / initrd SSH / TPM. |
| sops vs LUKS | **sops-nix does NOT unlock LUKS.** Activation is after root is up. Unlock material **never in git**. [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md). |
| Provider choice | Still open (**Q-HOST-1**, **Q-HOST-2**). |

---

## 5. Mail and data

| Item | Direction |
|------|-----------|
| Engine | **Stalwart** (SMTP, submission, IMAP, ManageSieve, JMAP, spam path, message store) |
| Stores | **RocksDB all-roles fine for now** (data + blob + FTS + in-memory/lookup co-located) |
| Search now | Stalwart **internal FTS** good enough; JMAP `Email/query` |
| Search later | Surmount builds **own search product**; do not add ES/Meilisearch for convenience |
| Migration | **MailPlus Maildir**, operator-run only ([MIGRATION.md](MIGRATION.md)); never silent activation import |
| Extra-domain mailboxes | Same local-part on another domain is **not** an alias. Distinct MailPlus accounts are separate User mailboxes. Living map: `~/.agents/surmount-server/operator-facts.md`. Password on services `/mail`, not SSH `update AccountPassword`. |
| Spam | **First-class** detection, greylist, lock-down, integrity |
| DNS legitimacy | **Earn trust on every mailbox domain** this host sends or receives through (primary `surmount.systems`, extra local domains such as `cryptoquick.com` and `baxterartworks.com`, later mailbox domains): SPF, dual DKIM TXT (Ed25519 + RSA-4096), DMARC (live mailbox policy is `p=quarantine`; do not use `p=reject`; `_dmarc.baxterartworks.com` intended is `p=quarantine`; public `_dmarc` may stay NXDOMAIN while EmailType is FWD), PTR/rDNS, MTA-STS, TLS-RPT, CAA, **and primary-class DNSSEC** (Namecheap hosted **DNSSEC Status ON**; ECDSA P-256 SHA-256 algorithm 13 acceptable for now). Working Namecheap hosted DNS when we are the registrar. No leftover parent DS without matching DNSKEY (that SERVFAILs validating resolvers; cryptoquick.com was this class). That leftover is a SERVFAIL **bug to clear then sign**, not a reason to leave the zone unsigned. We asked cryptoquick OFF only because leftover DS 2368 (alg 13, digest type 1 SHA-1) had no DNSKEY. Operator wants ON same as `surmount.systems`. Do **not** re-add 2368 by hand. DS digest type 1 (SHA-1) must FAIL even if a DNSKEY exists. Path: [DNS.md](DNS.md) *Best DNSSEC we can actually run*. Do **not** harden only the primary. Static-site-only extra vhosts are **not** automatically mail domains. Operator asked the primary public MX flip 2026-08-20 (dual-sign + PTR already green). `--live set-mx` is **not** a public MX flip while Email Forwarding is on; EmailType FWD fails closed. Leftover is Namecheap UI **Custom MX**, then re-run live `set-mx`. `list` prints EmailType. Laptop Namecheap ClientIp = laptop egress. [DNS.md](DNS.md). |
| Blob tiering / clever RPO-RTO | **Not now** |

Depth: [DATASTORES.md](DATASTORES.md),
[research/stalwart-0.16.15-stores-evidence.md](research/stalwart-0.16.15-stores-evidence.md).

---

## 6. Edge and security

| Item | Direction |
|------|-----------|
| Clearnet HTTPS | **Axum-first** edge (management-ui rustls on public **:80/:443**). Stalwart is **not** product clearnet HTTPS (P1). Free engine first-boot :443 via `nix/stalwart/free-public-443-for-axum-edge.ndjson`. nginx delete path. |
| :80 | Redirect/upgrade to :443. **Production (operator 2026-08-02):** port 80 is **free** for product **redirect-only** bind. **ACME HTTP-01 on product :80 parked** (Q-EDGE; free :80 does not invent ACME). Dual-run nginx may still use :80 for ACME while escape is on. |
| TLS | No SSLv3/1.0/1.1; **prefer TLS 1.3**; hybrid **PQ KEX** on rustls/aws-lc-rs (e.g. X25519MLKEM768) **first-class** (`prefer-post-quantum` + hermetic unit proof of default provider groups; **not** live host hybrid negotiation until `nix run .#surmount-tls-hybrid` after B1) |
| Deploy automation | Operator driver `nix run .#surmount-deploy-host` / `just deploy-host`; host-local contract [deploy-host-local.md](deploy-host-local.md); hermetic crate tests in `checks.*.ci` (not a remote switch). Security ladder B0-B7 host-gated. |
| PQConnect | **Separate** path-layer PQ between supporting peers; evaluate/plan; not a substitute for TLS hybrid alone |
| Local IPC | Prefer **Unix domain sockets**. Stalwart HTTP today is IP:port (UDS gap; loopback scaffold). |
| Access control | **Merciless ban** + **whitelist** (never banned) + **last-used**; edge rate limits; first path in UI + optional nft sets + **helper scaffold** (`nftHelper`, UI no CAP_NET_ADMIN); long-term **Rust + nftables**, not Python fail2ban as identity; Q-ACL-* open |
| No CF products | See absolute hygiene |
| **Arti hidden services** | **REQUIRED** (see section 7) |

Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md),
[SECURITY.md](SECURITY.md),
[research/access-control-fail2ban.md](research/access-control-fail2ban.md),
[research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md),
[research/rust-edge-and-uds.md](research/rust-edge-and-uds.md).

---

## 7. Arti hidden services (REQUIRED)

**Operator direction (2026-07-30 correction):** Surmount Server services must
be reachable via **Arti onion / hidden services**. That reachability is
**NOT optional.**

| Fact | Meaning |
|------|---------|
| **What** | **Arti** = Tor Project **Rust** Tor implementation ([upstream](https://gitlab.torproject.org/tpo/core/arti/)) |
| **Product surface** | First-class reachability via **onion/hidden services**, **alongside** clearnet where clearnet applies |
| **Not** | Not a replacement for the Axum clearnet HTTPS edge; not a CDN; not Cloudflare |
| **Still true** | Own stack; direct clearnet to VPS; no required CF hop |
| **Secrets** | HS identity keys and startup material = **deploy secrets** on host (**never in git**). Human inventory (onion address notes, recovery checklist) = **Vaultwarden**. Note: Vaultwarden does **not** implement Bitwarden **Secrets Manager** API today. |
| **Relay / egress** | Separate open choices (ToS, bandwidth). Required bar is **service reachability via onion HS**, not "must run a public exit relay." |
| **Code today** | **Management-publish module:** `modules/arti-hidden-service.nix` generates real `arti.toml` (`[onion_services]` + `proxy_ports`; backend derives from managementUi when unset; `state_dir` = host HS identity path). `startDaemon` default false; complete lean path does not need `acceptIncompleteOnionConfig` (no effect). **Surmount package:** `pkgs.artiOnionService` / `packages.*.arti-onion-service` is a **Surmount-owned source build of Arti 2.5.1** (GitLab `arti-v2.5.1`) with `onion-service-service` (distinct from stock `pkgs.arti` 1.4.2 client-default; often channel-lagged); rustc from flake input `nixpkgs-rust` for MSRV 1.91+. Module prefers it when `package` is null and treats `passthru.surmountOnionServiceCapable` as the capable claim. Stock path still fail-closed. **Local cleartext full API shipped** for https+Arti: `SURMOUNT_LOCAL_CLEARTEXT_LISTEN` / auto loopback when UI is https and Arti enable (onion rproxy targets cleartext HTTP or UDS; no TLS-on-onion). **Onion-Location + Alt-Svc shipped (2026-08-17; every public Host 2026-08-20)** on mapped HTTPS 2xx/3xx (apex, www, services, extra static Hosts, MTA-STS; same v3; `/_o/{host}` for non-console; process-start load; no hot-reload; optional `SURMOUNT_ONION_*` enable/disable/map env; dump on `GET /api/v1/system` `onion_discovery`). **Live host (2026-08-17):** unit `surmount-arti-hidden-service` active; durable HS dir; hostname file present; headers proven on HTTPS 307/200. **unit active != onion published** without live Tor Browser / onion-fetch verify. No private keys in tree. Residual: live Tor verify, operator offline HS backup, public-module HOME/`port_info`, Q-ARTI-* (do not invent answers). Do **not** claim B3 fully closed. Validation SoT: `just e2e` / `just check` / `checks.*.ci`. |

Research (update status to required):
[research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md).

Remaining shape questions (not "whether onion at all"): **Q-ARTI-2** ...
**Q-ARTI-4** in [open-choices.md](open-choices.md).

---

## 8. Identity and secrets

### Three custody domains (operator direction 2026-08-09; S0-S3 offline)

| Domain | Where | Job |
|--------|-------|-----|
| **A. Operator workstation** | OS secret store on laptop/admin machine. **Candidate:** GNOME Secret Service (libsecret / gnome-keyring). Operator uses `secret-tool` / Seahorse; **no** Surmount libsecret Rust crate. | Age admin keys, deploy-material values + labels, operator SSH. **Not** VPS boot/activation SoT. |
| **B. Host deploy secrets** | Files / systemd credentials / sops-nix (or Q-DEP-1 alt) on host at activation (e.g. `/run/surmount-secrets/...`) | Product consumes **env + file paths only**. Services start from host material. |
| **C. Human vault** | **Vaultwarden** (S7a offline; S7b enable path scripted; V4 human-store runbook) | Day-to-day passwords/TOTP; never feeds nixos-rebuild at activation |

**Bridge (S3-S4 offline):** A does **not** replace B.
`nix run .#secrets-install-host` materializes B paths from a **staging dir**
(primary) or optional `secret-tool` lookup. Opt-in S4:
`nix run .#surmount-deploy-host -- --install-secrets` may invoke the bridge before rebuild
(default off). Hermetic crate tests only in CI (`checks.*.ci`).
Never live keyring/host as green CI. S5 Rust helper parked; S6 host-gated
`requireDeployMaterial`; S7a domain C module offline shipped (sample enable=false;
**do not rebuild**); S7b enable path scripted by A0 host-cutover after token
(live unit host-gated). **V4 offline:** what to store in VW vs A/B
([SECRETS.md](SECRETS.md) section 5.1); optional
`nix run .#secrets-export-bw-to-staging` (C -> A staging only; never
activation; hermetic fixture/mock test). Not Bitwarden Secrets Manager.
Secrets **never** in project git.
Schema + inventory + runbook: [SECRETS.md](SECRETS.md) section 1 and 5.

### Two product buckets + disk (do not mash)

| Bucket / layer | Tool | Job |
|----------------|------|-----|
| **1. Deploy secrets** | sops-nix scaffold today (need real; tool can change) | Material on host at activation; **never in public git** (domain B) |
| **2. Humans** | **Vaultwarden** (S7a module offline done; S7b driver after token) | Passwords, TOTP, notes, mail cred inventory UX (password manager API) (domain C) |
| **Disk** | **LUKS2** | Offline FDE; not bucket 1 or 2; unlock never in git |
| Product auth | **Nostr** keys | Keys via host OS / other Surmount tools; not a password farm for users |
| Mail creds | Stalwart directory | Engine holds protocol auth; humans track via VW |
| Bitwarden Secrets Manager | Separate cloud SM product | **Do not** claim VW speaks SM API (`bws` / SM SDK) without new evidence |

Full: [SECRETS.md](SECRETS.md) (section 1 three domains), [SECURITY.md](SECURITY.md),
[operator-direction.md](operator-direction.md) section 11.

---

## 9. Product UI

| Item | Direction |
|------|-----------|
| Stack | **Axum + Leptos SSR** (invested); multi-page admin console (`/`, `/domains`, `/accounts`, `/system`, `/mail`) SSR-only; hydrate/webmail residual |
| Phase order | **Admin console first**, then **real webmail in v1** via JMAP |
| Legacy sites | **Static files only**, no exceptions (no PHP/Node for old Synology apps) |
| Longer-term clients | Desktop / local-first preference noted as complementary; does not cancel v1 webmail |
| Stalwart `/admin` | Bootstrap fallback (local / tunnel); not long-term public primary |

[SEARCH_AND_UI.md](SEARCH_AND_UI.md).

---

## 10. Packages and forks

| Item | Direction |
|------|-----------|
| Overlay | **Surmount package overlay** L1 in this repo |
| Stalwart | Prefer current via FODs; **SurmountSystems/stalwart** when patches needed; **flake input** consume |
| PQConnect | Sibling path flake after **human** commit; not wired into surmount-server yet. Rebuild on **nixos-26.05** before readiness. Keys host-only / never git. Do not mash with D1 hybrid TLS. |
| RocksDB | System ABI goal on **source** build; binary FOD bridge today |
| Agents | Never commit/push forks unless explicitly ordered; leave uncommitted packaging for human |

[packages-and-forks.md](packages-and-forks.md).

---

## 11. Doc index (major paths)

### Living law / maps

| Path | One-line purpose |
|------|------------------|
| [COMPACTION-PIN.md](COMPACTION-PIN.md) | **This file** - reload after compaction |
| [operator-direction.md](operator-direction.md) | Dated operator direction dump + follow-ups |
| [principles.md](principles.md) | Engineering principles (hermetic, versions, security, stack language) |
| [hygiene.md](hygiene.md) | Standing operational rules; top rule never secrets in git |
| [glossary.md](glossary.md) | FOD, Day-1/Day-2, Fix, deploy secrets, Arti, ... |
| [STACK.md](STACK.md) | Living architecture map |
| [open-choices.md](open-choices.md) | Still open / proposed; Q-* catalog |
| [packages-and-forks.md](packages-and-forks.md) | Overlay, FODs, fork consume, agent git rules |
| [fix-and-fixos.md](fix-and-fixos.md) | Fix / FixOS / fixpkgs ladder |
| [EDGE_AND_TLS.md](EDGE_AND_TLS.md) | Edge, TLS, rate limits, Axum-first, Arti HS, no CF |
| [SECURITY.md](SECURITY.md) | Threat model, Nostr, LUKS, posture |
| [SECRETS.md](SECRETS.md) | Three custody domains; two buckets + LUKS; VW vs Secrets Manager |
| [DATASTORES.md](DATASTORES.md) | Durable store inventory + Stalwart storage |
| [SEARCH_AND_UI.md](SEARCH_AND_UI.md) | Search split; admin then webmail phases |
| [DNS.md](DNS.md) | Earn-trust mail DNS checklist |
| [MIGRATION.md](MIGRATION.md) | MailPlus Maildir import runbook |
| [OPS.md](OPS.md) | Logs, health, scripts, self-ops |
| [deploy-host-local.md](deploy-host-local.md) | Host-local overlay contract + deploy driver + B0-B7 ladder |
| [architecture-review.md](architecture-review.md) | Peer review of tree facts (partially superseded) |
| [../AGENTS.md](../AGENTS.md) | Agent standing law for this repo |
| [../README.md](../README.md) | Entry + apply flake |
| [../secrets/README.md](../secrets/README.md) | Deploy-secrets bootstrap docs (no real secrets) |

### Research (depth; not living law alone)

| Path | One-line purpose |
|------|------------------|
| [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md) | Arti HS required + VW vs Bitwarden SM |
| [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md) | TLS hybrid PQ + PQConnect; CF Research cites |
| [research/pqconnect-local-packaging.md](research/pqconnect-local-packaging.md) | Sibling PQConnect Nix flake status |
| [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md) | LUKS unlock vs sops; never unlock in git |
| [research/access-control-fail2ban.md](research/access-control-fail2ban.md) | Merciless ban + whitelist design |
| [research/rust-edge-and-uds.md](research/rust-edge-and-uds.md) | Axum edge candidates; Stalwart UDS gap |
| [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md) | Cert trust paths; not ACME-only locked |
| [research/stalwart-0.16.15-stores-evidence.md](research/stalwart-0.16.15-stores-evidence.md) | Current engine store evidence |
| [research/stalwart-stores-evidence-2026-07-30.md](research/stalwart-stores-evidence-2026-07-30.md) | Historical 0.11.8 only |
| [research/version-audit.md](research/version-audit.md) | Pin vs latest audit (re-verify) |
| [research/fix-fixos-ladder.md](research/fix-fixos-ladder.md) | Deep Fix/FixOS notes |
| [research/scaffold-assumptions-inventory.md](research/scaffold-assumptions-inventory.md) | Scaffold assumption list |

Reports (session handoffs, not law): `~/.agents/reports/` on this machine.
Call those handoffs **reports**, never "joins". Do not write new handoffs
under repo `.agents/` or `.grok/joins/`. Do not recreate repo
`.agents/plans`, `.agents/reports`, or `.agents/joins`. Do not create
project-root `.grok/` for reports, plans, or scratch. Product scripts and
tests must not mkdir those leftover homes; use `mktemp` / `$TMPDIR`, or
host `~/.agents/reports/` for notes. Leftover repo
`.grok/joins/` reports were moved 2026-08-18 to `~/.agents/reports/`.

---

## 12. Open questions (do not invent answers)

Full context: [open-choices.md](open-choices.md),
[operator-direction.md](operator-direction.md) section 10.

| Id | Topic |
|----|-------|
| **Q-HOST-1** | Exact VPS provider and plan SKU (do not invent names) |
| **Q-HOST-2** | LUKS2 from day one vs interim plain disk |
| **Q-LUKS-1** ... **Q-LUKS-4** | Unlock pattern; initrd story; keyfile weakness; full root vs data-only |
| **Q-TLS-1** ... | Cert trust path browser HTTPS vs mail TLS; DANE timing |
| **Q-CA-1** / **Q-CA-2** | Primary public CA; multi-CA failover on one VPS |
| **Q-PQC-1** ... **Q-PQC-4** | PQConnect Day-1 vs Day-2; client audience; mail PQ path; package timing |
| **Q-ARTI-2** | Which surfaces first over onion (private HTTP, mail, etc.) |
| **Q-ARTI-3** | Onion exposure of Stalwart admin/JMAP? Default lean: no public onion mail admin without explicit yes |
| **Q-ARTI-4** | Provider ToS / abuse if non-exit relay shares MX IP |
| **Q-SEC-SM-1** / **Q-SEC-SM-2** | True Secrets Manager API need vs deploy secrets + VW PM; if SM required, wait VW / commercial / Surmount Rust service |
| **Q-ACL-1** ... **Q-ACL-6** | Unauthorized definition; whitelist store; ban duration; Rust guard timing; IPv6; mail AUTH vs global ban |
| **Q-EDGE-1** / **Q-EDGE-2** | Shared cert files vs in-process; UDS path layout |
| **Q-AUTH-1** | Session store, first-operator bootstrap allowlist, key-loss recovery |
| **Q-DEP-1** | Keep sops-nix long-term vs other deploy-secrets tool (need stays; git storage stays forbidden) |

**Answered direction (do not re-open without operator):** Axum-first edge; no
nginx product; RocksDB OK now; internal FTS OK now; Nostr product auth;
Vaultwarden for humans; deploy secrets at activation; never secrets in git;
no Python/NPM product; no CF products; **Arti HS required**; single VPS until
operator says; SurmountSystems/stalwart flake-input pattern; poolWorkers leave
default on directed box.

---

## 13. Explicit non-claims (not implemented yet)

Honest gaps. Do not claim these are shipping code:

| Gap | Reality today |
|-----|----------------|
| **Arti live Tor publish** | Module generates **management-publish** `arti.toml` (onion + rproxy, no keys in tree); Surmount `artiOnionService` is owned **2.5.1** source + `onion-service-service` (not nixpkgs 1.4.2 lag); stock path fail-closed. **Local cleartext full API for https+Arti auto-path shipped** (loopback `SURMOUNT_LOCAL_CLEARTEXT_LISTEN` + Arti backend target; lean onion backend is cleartext HTTP or UDS). **Onion status product-real (2026-08-10):** Arti enable derives hostname file + HS state dir env; structured `configured` / `hostname_missing` / `not_provisioned` on SSR + `GET /api/v1/system`; residual names `surmount.artiHiddenService` (not local demo). Lab `SURMOUNT_ONION_URL` only. **Onion-Location + Alt-Svc shipped (2026-08-17; every public Host 2026-08-20)** on mapped HTTPS 2xx/3xx (apex, www, services, extra static Hosts, MTA-STS; same v3; `/_o/{host}` for non-console; process-start load; no hot-reload; optional `SURMOUNT_ONION_LOCATION_ENABLED` / `SURMOUNT_ONION_ALT_SVC_ENABLED` / `SURMOUNT_ONION_*_DISABLED_HOSTS` / `SURMOUNT_ONION_MAP_FILE`; dump on `GET /api/v1/system` `onion_discovery`). **Live host (2026-08-17, private host-local):** unit `surmount-arti-hidden-service` active; durable HS dir (not `/run`); hostname file present (v3 onion; do not paste the address in this public tree); headers proven on HTTPS 307/200. Host-local `HOME=/var/lib/surmount/arti`; `surmount-ui` in group `surmount-arti`; keystore 0700. Still residual: **live Tor Browser / onion-fetch verify**, operator offline backup of HS identity, public-module HOME/`port_info`. Do **not** claim onion published from unit active or header presence alone; local temp-key green != operator backup. Do **not** claim B3 fully closed. Do not invent Q-ARTI-2/3 answers |
| **Axum HTTPS public cutover / nginx delete** | In-process rustls TLS 1.3 acceptor wired (host PEMs, fail-closed key mode). **`surmount.web.enable` default false** (nginx not product edge). Dual-run escape: `web.enable = true`. Module/nginx code still in tree. **:80 redirect-only bind wired** when `redirectHttpToHttps` + listen (dual-run mutex at eval; no cleartext API on :80). **Operator 2026-08-02:** production port 80 is **free** for that redirect-only path. **ACME HTTP-01 on product :80 parked** (Q-EDGE; do not invent ACME-on-product-:80). **In-process ACME scaffold (2026-08-10; A1/A2):** `instant-acme` + DNS-01 inside management-ui, **default off**; static PEMs still first-class; hermetic `mock` + **external-hook** DNS-01 challenge adapter (operator-owned executable; not a commercial DNS brand); early-renew default 30 days. **Live host (2026-08-11 free-443; 2026-08-12 mta-sts + policy):** public :443 is **surmount-manage** not Stalwart; `https://services.surmount.systems/health` **200** without `-k`. Cert is **Let's Encrypt production** (issuer YE1; browser-trusted). Certificate hostnames **17** (none dropped, 2026-08-20): six extra static zones apex+www (`baxterartworks.com`, `btcfur.com`, `exophiles.org`, `iantuckerstudios.com`, `nostrfurs.com`, `yiffa.app`) plus `surmount.systems`, `www.surmount.systems`, `mail.surmount.systems`, `services.surmount.systems`, `mta-sts.surmount.systems`. Extra-vhost HTTPS for those six is **live**. Do **not** add `cryptoquick.com` / `www.cryptoquick.com` to that leaf until validating lookups succeed (live 2026-08-20 leftover DS 2368 still SERVFAIL; hosted DNSSEC ON is the goal; not this turn). Mail **is** on the production leaf (IMAP/SMTPS 993/465 YE1). Issued by **laptop DNS-01** (Namecheap; ClientIp = laptop egress; SettleSeconds=120). Host ACME stays **off**. Durable PEMs under `/var/lib/surmount/secrets/tls/`. Named renew: `just laptop-renew-cert` (`--directory` required; do **not** re-issue just to add a name already on the leaf). Apex/www public page is packaged **SurmountSystems/site** (live 2026-08-19 tip `1c84696`; not UNDER CONSTRUCTION). Private host-local `mtaStsMode=testing`; public `https://mta-sts.<apex>/.well-known/mta-sts.txt` **200** `mode: testing` (stay testing while DNS MX is eforward; do not enforce). Reports: `../.agents/reports/impl-live-le-production-laptop.md`, `../.agents/reports/impl-earn-trust-wave.md`. **Post-reboot proof PASS (2026-08-13).** Later 2026-08-18 hard cycle: health/apex **200**, IMAP Let's Encrypt including **mail**. Agents never reboot. Residual: firewall across switch, MemoryDenyWriteExecute host proof (`just e2e-host`). Day-one order: [OPS.md](OPS.md). Do **not** claim live onion or "nginx deleted from repo" without further host proof. Do **not** invent MX / DMARC / VW. Agent language: say **certificate hostnames**, not bare **SAN** (collides with storage SAN). Dual-run unused checklist: [OPS.md](OPS.md); **do not delete `modules/web.nix` without operator OK** |
| **Merciless ban + whitelist** | First path + helper scaffold: Rust ban decide + memory/file; optional kernel firewall sync via **socket-activated** `surmount-nft-ban-helper@` (Unix socket `SURMOUNT_BAN_NFT_HELPER_SOCK`; CAP_NET_ADMIN on helper unit only) or unsupported direct `nftExec`; UI keeps NNP and **no** CAP_NET_ADMIN (child setcap spawn is not the product path). DryRun does not mutate sets; Enforce apply-then-durable. Hermetic mock/tests under `just e2e`. **`remove_ban` / lab unban shipped** (helper `remove_ban` / CLI `remove-ban`; delete-element; absent element = ok). **Auth-failure BanCandidate + matrix shipped** (`decide_ban_signal` / `BanGuard::signal_unauthorized` + request-context hook; whitelist immune; Off/DryRun/Enforce). Session exchange fail and bad presented NIP-98 may signal once; missing cookie does not; 404/501 still do not auto-ban. Table: [SECURITY.md](SECURITY.md). **Not** live host bans / full Q-ACL-1..6 surface answers. Live drop: `just e2e-host` + sets `surmount-ban4`/`surmount-ban6`. fail2ban sshd still transitional sketch |
| **Leptos SSR admin/webmail** | **Multi-page console + UI depth (2026-08-01; directory live 2026-08-07):** SSR `/`, `/domains`, `/accounts`, `/system`, `/mail`, `/login` (DOGE only; no skeleton). Inventory cards, onion chip, system definition table, mail probe card. Accounts honest empty by default; domains config inventory. **Directory trait + hermetic mock + live Stalwart client shipped** (`AppState.directory`; `unavailable` default; labeled `mock` never default-on; `stalwart` = management JMAP `x:Account/query`+`get` via Bearer token, explicit `SURMOUNT_DIRECTORY=stalwart` + host token only; list fail-closed empty). **JMAP thin 501 boundary locked (2026-08-07):** `POST /api/v1/jmap` honest 501 (`jmap_proxy_not_implemented`); mail SSR residual copy; no `/webmail` product route; hermetic anchors. Directory research: [research/stalwart-directory-api.md](research/stalwart-directory-api.md). **Account create/update API mutations shipped (2026-08-07)** (`POST/PATCH /api/v1/accounts`; auth gate + CSRF; mock + `x:Account/set` wire-mock). **Create mailbox form shipped (2026-08-14)** on `/mail` (Administrator; optional npub; User vs Administrator; map at `/var/lib/surmount/console/accounts.json` or `SURMOUNT_CONSOLE_ACCOUNTS`). **Attach npub shipped (2026-08-20):** Administrator `/mail` Grant console login pastes bech32 `npub1...`, session-bound CSRF, binds an existing mailbox so that person can log into the services portal (AuthMode nostr). Garbage/nsec refused. Directory listing not required. `/accounts` still has no create form. **Structured request logging shipped.** Still residual: hydrate islands only if clear SSR gap (parked), full Q-AUTH-1, JMAP proxy + webmail **beyond** 501, **MX still parked**, **host cutover**. Living crane rustc **1.95** (`rustPackages_1_95`); Leptos MSRV floor is >=1.88 |
| **Nostr auth end-to-end** | **Foundation shipped (2026-08-01; polish 2026-08-02):** rust-nostr NIP-98 + HMAC session cookie scaffold; `SURMOUNT_AUTH_MODE` off/nostr; env allowlist + optional allowlist file (env wins; empty fail-closed); challenge/session/me/logout + login page. **Not JS NDK.** UI honesty (auth-mode banners, `/login` when nostr). Hermetic e2e anchors list off/gate/NIP-98 session + surface audit + ban matrix (and onion unset residual). CSRF on account mutations + structured request logging shipped (2026-08-07). **Account mutation e2e hermetic anchors shipped (2026-08-07):** auth-off fail-closed, cookie CSRF (incl. PATCH), lab escape mock, unavailable 503. **Live B4 (2026-08-12):** public services console is Nostr-gated (anon `/` login, `/api/v1/domains` 401, `/health` 200); apex/www packaged **SurmountSystems/site** (2026-08-19 tip `1c84696`). Report: `../.agents/reports/impl-auth-live-b4-switch.md`. Q-AUTH-1 residual: key-loss, durable session store, first-operator bootstrap UX (do not invent). nsec never on server. |
| **Vaultwarden module** | **S7a shipped offline:** `modules/vaultwarden.nix`, management console URL/chip/Open vault, pure eval tests. Sample host enable=false. **Do not rebuild S7a.** **S7b** A0 scripts install kind + private enable fragment + smoke; live unit after operator token + target. **V4 offline:** human-store runbook + optional `secrets-export-bw-to-staging` (not activation). **Phase B offline (2026-08-11):** Axum `/vault/` reverse-proxy in management-ui (`vaultwardenProxyEnable` default off; hermetic green); not live host VW; not public HTTPS. Not SM API |
| **Operator OS secret store + deploy bridge** | Direction 2026-08-09; **S0-S4 + A0 cutover pack shipped offline:** living contract + runbook + `nix run .#secrets-install-host` + opt-in `nix run .#surmount-deploy-host -- --install-secrets` + `nix run .#surmount-host-cutover` compose (inventory, lab DNS hook, fragments) + hermetic crate tests. **Not** shipped: libsecret Rust crate, live keyring CI, S5 Rust helper, S6 live requireDeployMaterial with real values, live S7b unit / Q-SEC-SM-*. Host still needs B0+ files on box |
| **LUKS install on live VPS** | Posture docs; provider + install path open |
| **Stalwart flake input to fork** | Pattern documented; binary FOD path still primary |
| **PQConnect in surmount-server flake** | Sibling packaging uncommitted (human owns commit); path input not wired by default; 26.05 rebuild before readiness; keys never in git |
| **System rocksdb link** | Design goal; binary embeds RocksDB |
| **UDS to Stalwart HTTP** | Engine listener model IP:port on pin; loopback scaffold |
| **Production MailPlus import** | Living uid-to-person map, aliases, and import counts: `~/.agents/surmount-server/operator-facts.md`. Primary hunter mailbox imported from DS1513 (Vandelay; `.All Mail` excluded). Extra people from DS3018xs are ordinary **User** only (no console Administrator). Extra-domain same-local-part mailboxes are **not** aliases. Extra public MX not flipped. Do not alias `admin@surmount.systems` onto a person mailbox. Do not serve unowned domains. Copy helper `--host` matches IPv4 GVFS via laptop-private hint; `gio list` even when libc `[[ -d cur ]]` fails on AFP. Password on services `/mail`, not SSH `AccountPassword`. |
| **Own search product** | Future; internal FTS only now |
| **Multi-host** | Deferred |

Tree that **does** exist: flake, Surmount Stalwart FODs + service module,
transitional nginx web module (default off; dual-run escape), management-ui
Axum edge (rustls HTTPS acceptor + TLS path config, rate limit, ban decide
layer default off, **local cleartext full API** for https+Arti, redirect
helpers + optional redirect-only :80 bind; ACME-on-:80 parked; in-process
ACME DNS-01 scaffold default-off (`instant-acme`, external-hook + early-renew);
**Leptos SSR
multi-page management console** (`/`, `/domains`, `/accounts`, `/system`,
`/mail`); **nft ban helper scaffold** UDS + socket-activated oneshot with
`add_ban` / `remove_ban` / `ping`), Arti HS management-publish module
(config + optional daemon; auto cleartext backend when UI https),
deploy-secrets fail-loud options, hardening/networking + optional
accessControl nft sets, docs, scripts, local/host end-to-end harnesses
(`nix run .#e2e` / `nix run .#e2e-host`, thin `just e2e` / `just e2e-host`),
VM and pure contract checks. Crane management-ui uses host-channel
`rustPackages_1_95` (nixos-26.05). E2E SoT is flake apps + Rust `crates/surmount-e2e`
(not free-floating bash). Host e2e is never in `checks.*.ci`. Tor deep row is
optional on local `just e2e` only (not CI). Full hermetic anchors live outside
the aggregate today; folding **hermetic-only** into a flake check is a product
choice (cost vs safety), not required by physics. Never treat green CI as host
cutover or live onion. Validation SoT: [RESIDUAL.md](../RESIDUAL.md).

---

## 14. How to use this file after compaction

1. Read **this pin** top to bottom if context was lost.
2. For the task at hand, open **one** child doc (do not reload the whole tree
   into the parent chat).
3. Multi-file work: spawn subagents; write short reports under
   `~/.agents/reports/`.
4. Same-turn disk update when direction changes; chat is not the system of
   record.
5. Re-verify version pins before packaging claims.
6. Never invent secrets examples with real values. Never commit secrets.

---

## Related reports

Older session reports that used to live under repo `.grok/joins/` now live
on this machine under `~/.agents/reports/` (`arti-required-compaction-pin.md`
and same-day notes such as `arti-pqc-mail-docs.md`,
`hygiene-no-git-secrets.md`, `security-pqc-access.md`, `pqconnect-nix.md`).
Those notes may predate the **Arti required** correction. **This file and
living docs win** over older report prose that said Arti was optional.
