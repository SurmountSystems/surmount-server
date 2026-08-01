# Compaction pin (reload after context loss)

**Single reload file** for humans and agents after chat compaction.
Read this first, then follow links into child docs. Do not invent product
scope from memory alone.

**Last updated:** 2026-07-31
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
| **Agents never `git commit`** | Human-signed only. No agent push. |
| **Forks** | Agents never touch `SurmountSystems/*` fork git unless the operator **explicitly** orders that work. Stalwart fork consume: flake input. PQConnect Nix work on sibling repo stays **unstaged for human** commit. |
| **Always validate versions** | Re-check upstream latest when touching pins. Docs lag. [research/version-audit.md](research/version-audit.md). |
| **Prove assumptions** | No "unsafe" / scare copy without evidence (failed build, broken import, measured issue). |
| **Question ids** | Unique global **Q-*** (or per-file Q1, Q2...). Never collide with section numbers. |
| **Hierarchical subagents** | Multi-file research/impl: coordinator thin; workers own greps/edits; join under `.grok/joins/`. |
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
| stalwart-cli | **1.0.12** | At latest |
| Stalwart WebUI | **1.0.7** | At latest |
| spam-filter rules | **3.0.0** | At latest |
| Arti (HS package) | Surmount-owned source **2.5.0** (`arti-onion-service.nix`, GitLab `arti-v2.5.0`) + `onion-service-service` | Not stock nixpkgs 1.4.2 lag; rustc via `nixpkgs-rust` (MSRV 1.91+). Live Tor verify still residual |
| management-ui rustc (crane) | **1.88.0** via `rustPackages_1_88` | Leptos MSRV; channel default rustc **1.86.0**; host/stable rustc often newer (e.g. 1.97.x) |
| Host nixpkgs channel | **nixos-25.05** lock | Engine is **not** from channel; channel lag is separate |
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
| Size | **~16 GB RAM**, **2 TB NVMe**, **16 logical cores** (32 cores optional later note, not a requirement) |
| Provider | **Operator-chosen VPS**, not Hetzner-as-default. **Do not invent a provider name.** Assume NixOS allowed; operator confirms NixOS + LUKS2 with provider. |
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
| Spam | **First-class** detection, greylist, lock-down, integrity |
| DNS legitimacy | **Earn trust:** SPF, DKIM, DMARC, PTR/rDNS, MTA-STS, TLS-RPT; plan DNSSEC + DANE/TLSA. Automate affordable certs; custom CA **not locked**. [DNS.md](DNS.md). |
| Blob tiering / clever RPO-RTO | **Not now** |

Depth: [DATASTORES.md](DATASTORES.md),
[research/stalwart-0.16.15-stores-evidence.md](research/stalwart-0.16.15-stores-evidence.md).

---

## 6. Edge and security

| Item | Direction |
|------|-----------|
| Clearnet HTTPS | **Axum-first** edge (in-process or small `surmount-edge` crate). nginx delete path. |
| :80 | Redirect/upgrade to :443 (ACME HTTP-01 may share :80) |
| TLS | No SSLv3/1.0/1.1; **prefer TLS 1.3**; hybrid **PQ KEX** on rustls/aws-lc-rs (e.g. X25519MLKEM768) **first-class** |
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
| **Code today** | **Management-publish module:** `modules/arti-hidden-service.nix` generates real `arti.toml` (`[onion_services]` + `proxy_ports`; backend derives from managementUi when unset; `state_dir` = host HS identity path). `startDaemon` default false; complete lean path does not need `acceptIncompleteOnionConfig` (no effect). **Surmount package:** `pkgs.artiOnionService` / `packages.*.arti-onion-service` is a **Surmount-owned source build of Arti 2.5.0** (GitLab `arti-v2.5.0`) with `onion-service-service` (distinct from stock `pkgs.arti` 1.4.2 client-default on nixos-25.05); rustc from flake input `nixpkgs-rust` for MSRV 1.91+. Module prefers it when `package` is null and treats `passthru.surmountOnionServiceCapable` as the capable claim. Stock path still fail-closed. **Local cleartext full API shipped** for https+Arti: `SURMOUNT_LOCAL_CLEARTEXT_LISTEN` / auto loopback when UI is https and Arti enable (onion rproxy targets cleartext HTTP or UDS; no TLS-on-onion). **unit active != onion published** without live Tor verify + operator host HS keys. No private keys in tree. Residual: live Tor verify, operator host keys/ownership, hardening after real `arti proxy`, Q-ARTI-* (do not invent answers). Validation SoT: `just e2e` / `just check` / `checks.*.ci`. |

Research (update status to required):
[research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md).

Remaining shape questions (not "whether onion at all"): **Q-ARTI-2** ...
**Q-ARTI-4** in [open-choices.md](open-choices.md).

---

## 8. Identity and secrets

| Bucket / layer | Tool | Job |
|----------------|------|-----|
| **1. Deploy secrets** | sops-nix scaffold today (need real; tool can change) | Material on host at activation; **never in public git** |
| **2. Humans** | **Vaultwarden** (planned) | Passwords, TOTP, notes, mail cred inventory UX (password manager API) |
| **Disk** | **LUKS2** | Offline FDE; not bucket 1 or 2; unlock never in git |
| Product auth | **Nostr** keys | Keys via host OS / other Surmount tools; not a password farm for users |
| Mail creds | Stalwart directory | Engine holds protocol auth; humans track via VW |
| Bitwarden Secrets Manager | Separate cloud SM product | **Do not** claim VW speaks SM API (`bws` / SM SDK) without new evidence |

Full: [SECRETS.md](SECRETS.md), [SECURITY.md](SECURITY.md).

---

## 9. Product UI

| Item | Direction |
|------|-----------|
| Stack | **Axum + Leptos SSR** (invested); admin `GET /` is Leptos SSR scaffold (ssr-only); hydrate/webmail residual |
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
| PQConnect | Sibling path flake after **human** commit; not wired into surmount-server yet |
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
| [SECRETS.md](SECRETS.md) | Two buckets + LUKS; VW vs Secrets Manager |
| [DATASTORES.md](DATASTORES.md) | Durable store inventory + Stalwart storage |
| [SEARCH_AND_UI.md](SEARCH_AND_UI.md) | Search split; admin then webmail phases |
| [DNS.md](DNS.md) | Earn-trust mail DNS checklist |
| [MIGRATION.md](MIGRATION.md) | MailPlus Maildir import runbook |
| [OPS.md](OPS.md) | Logs, health, scripts, self-ops |
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

Joins (session handoffs, not law): `.grok/joins/`.

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
| **Arti live Tor publish** | Module generates **management-publish** `arti.toml` (onion + rproxy, no keys in tree); Surmount `artiOnionService` is owned **2.5.0** source + `onion-service-service` (not nixpkgs 1.4.2 lag); stock path fail-closed. **Local cleartext full API for https+Arti auto-path shipped** (loopback `SURMOUNT_LOCAL_CLEARTEXT_LISTEN` + Arti backend target; lean onion backend is cleartext HTTP or UDS). Local optional Tor row (`just e2e`) may use temp keys when arti present. Still residual: **live Tor verify on host**, operator HS keys/ownership under `surmount-arti`, hardening after real `arti proxy`. Do **not** claim onion published from unit active alone; local temp-key green != host ownership. Do not invent Q-ARTI-2/3 answers |
| **Axum HTTPS public cutover / nginx delete** | In-process rustls TLS 1.3 acceptor wired (host PEMs, fail-closed key mode). **`surmount.web.enable` default false** (nginx not product edge). Dual-run escape: `web.enable = true`. Module/nginx code still in tree. **:80 redirect-only bind wired** when `redirectHttpToHttps` + listen (dual-run mutex at eval; no cleartext API on :80). **ACME HTTP-01 on product :80 parked** (Q-EDGE; do not invent ACME-on-product-:80). Local e2e covers HTTPS with **self-signed** temp PEMs (`just e2e`). Host public :443 + MemoryDenyWriteExecute (no writable+executable memory) + cert path still operator residual (`just e2e-host`). Do not claim live public cutover, live onion, host MemoryDenyWriteExecute hardening done, or "nginx deleted from repo" without host proof. Dual-run unused checklist: [OPS.md](OPS.md) (nginx dual-run unused detection); **do not delete `modules/web.nix` without operator OK** |
| **Merciless ban + whitelist** | First path + helper scaffold: Rust ban decide + memory/file; optional kernel firewall sync via **socket-activated** `surmount-nft-ban-helper@` (Unix socket `SURMOUNT_BAN_NFT_HELPER_SOCK`; CAP_NET_ADMIN on helper unit only) or unsupported direct `nftExec`; UI keeps NNP and **no** CAP_NET_ADMIN (child setcap spawn is not the product path). DryRun does not mutate sets; Enforce apply-then-durable. Hermetic mock/tests under `just e2e`. **`remove_ban` / lab unban shipped** (helper `remove_ban` / CLI `remove-ban`; delete-element; absent element = ok). **Auth-failure BanCandidate stub shipped** (`decide_ban_signal` / `BanGuard::signal_unauthorized` + request-context hook; whitelist immune; Off/DryRun/Enforce). **Not** live host bans / Q-ACL-1..6 surface answers / Nostr auth (Q-AUTH-1 parked; do not invent). Live drop: `just e2e-host` + sets `surmount-ban4`/`surmount-ban6`. fail2ban sshd still transitional sketch |
| **Leptos SSR admin/webmail** | **Admin shell scaffold landed:** `GET /` is Leptos SSR (ssr-only, no hydrate/WASM/NPM) through shared Axum stack. Richer admin pages, hydrate, Nostr auth, JMAP/webmail UI still residual. Crane rustc 1.88 for Leptos MSRV |
| **Nostr auth end-to-end** | Direction; product verify/session not complete (Q-AUTH-1 parked) |
| **Vaultwarden module** | Planned; not wired as Surmount product module |
| **LUKS install on live VPS** | Posture docs; provider + install path open |
| **Stalwart flake input to fork** | Pattern documented; binary FOD path still primary |
| **PQConnect in surmount-server flake** | Sibling packaging uncommitted; path input not wired |
| **System rocksdb link** | Design goal; binary embeds RocksDB |
| **UDS to Stalwart HTTP** | Engine listener model IP:port on pin; loopback scaffold |
| **Production MailPlus import** | Runbook + helper template; not proven on live corpus |
| **Own search product** | Future; internal FTS only now |
| **Multi-host** | Deferred |

Tree that **does** exist: flake, Surmount Stalwart FODs + service module,
transitional nginx web module (default off; dual-run escape), management-ui
Axum edge (rustls HTTPS acceptor + TLS path config, rate limit, ban decide
layer default off, **local cleartext full API** for https+Arti, redirect
helpers + optional redirect-only :80 bind; ACME-on-:80 parked; **Leptos SSR
admin shell** on `GET /`; **nft ban helper scaffold** UDS + socket-activated
oneshot with `add_ban` / `remove_ban` / `ping`), Arti HS management-publish
module (config + optional daemon; auto cleartext backend when UI https),
deploy-secrets fail-loud options, hardening/networking + optional
accessControl nft sets, docs, scripts, local/host end-to-end harnesses
(`nix run .#e2e` / `nix run .#e2e-host`, thin `just e2e` / `just e2e-host`),
VM and pure contract checks. Crane management-ui uses rustc 1.88 (not channel
default 1.86) for Leptos. E2E SoT is flake apps + Rust `crates/surmount-e2e`
(not free-floating bash). Host e2e is never in `checks.*.ci`.
Validation SoT: [RESIDUAL.md](../RESIDUAL.md).

---

## 14. How to use this file after compaction

1. Read **this pin** top to bottom if context was lost.
2. For the task at hand, open **one** child doc (do not reload the whole tree
   into the parent chat).
3. Multi-file work: spawn subagents; join under `.grok/joins/`.
4. Same-turn disk update when direction changes; chat is not the system of
   record.
5. Re-verify version pins before packaging claims.
6. Never invent secrets examples with real values. Never commit secrets.

---

## Related joins

- `.grok/joins/arti-required-compaction-pin.md` (this correction pass)
- Prior same-day joins under `.grok/joins/` (arti-pqc-mail-docs, hygiene,
  security-pqc-access, pqconnect-nix, ...) may predate the **Arti required**
  correction; **this file and living docs win** over older join prose that
  said Arti was optional.
