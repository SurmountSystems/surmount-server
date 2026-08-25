# Operator direction (2026-07-30)

Dated dump of product direction the operator stated for Surmount Server.
Treat as **working product direction** labeled by date, not as eternal law
that can never change. Stronger than scaffold defaults when they conflict.
Still not every line of research prose in the tree.

**Status label:** operator direction 2026-07-30 (plus follow-up answers same day)
**Compaction reload:** [COMPACTION-PIN.md](COMPACTION-PIN.md) (read first after
context loss).
**Companions:** [principles.md](principles.md), [glossary.md](glossary.md),
[packages-and-forks.md](packages-and-forks.md), [open-choices.md](open-choices.md),
[STACK.md](STACK.md), [SECRETS.md](SECRETS.md), [EDGE_AND_TLS.md](EDGE_AND_TLS.md).

When this file and older scaffold docs disagree, update the living docs and
mark scaffold pieces transitional. Do not silently keep invented provider
defaults or "Caddy target" copy if the operator restated otherwise.

---

## 1. Host

| Item | Direction |
|------|-----------|
| Form factor | Single **operator-chosen VPS** |
| Size / plan | **Open (Q-HOST-1).** Do **not** invent or publish RAM, disk, core counts, or provider SKUs as product law. Operator picks the machine. |
| Provider | **Operator-chosen VPS.** Prefer a provider where PTR/rDNS and LUKS install path are workable. **Do not invent a provider name.** Operator will ask the chosen provider about NixOS + LUKS2 recommendations. **Assume** the provider allows NixOS. |
| Load | Minimize idle load on the node; use cores when useful (compaction, FTS, builds, spam). Do not run extra always-on agents "because cores exist." |
| Topology end | Single VPS ends **when the operator says**. Do not invent scale-out triggers. Do not cite other projects as exit criteria. |

### Host capacity (no public size claim)

Capacity is whatever the operator buys. Public docs and agent prose must not
assert a fixed RAM/disk/CPU SKU. Size RocksDB and other knobs from **upstream
defaults + measurement on the real host**, not from an invented plan.

---

## 2. Datastore

| Item | Direction |
|------|-----------|
| Engine | **RocksDB is fine for now** |
| Layout | All-role co-location OK (data + blob + FTS + in-memory/lookup on one RocksDB) |
| Blob tiering / compliance split | **Not now** (later other work, not this phase) |
| In-memory on RocksDB | **Fine for now** |
| Clever RPO/RTO backup design | **Later, not now** |
| Priority | Recover **Synology MailPlus** data onto self-sovereign Surmount Server (NixOS / FixOS direction), hermetic modern flakes |

### Internal FTS (what it is; pin for now)

**Internal FTS** = Stalwart's built-in full-text search index in the search
store role. On our layout that index lives in the same RocksDB as metadata
(Default search store). Clients search via JMAP `Email/query` or IMAP SEARCH.
Surmount product code does **not** open RocksDB for search.

| Question people ask | Answer |
|---------------------|--------|
| Is internal FTS "real" search? | Yes enough for mail ops and v1 webmail: subject/body token index maintained by the engine. |
| Is it Elasticsearch? | No. No separate search cluster. |
| Good enough now? | **Yes. Good enough for now** (operator direction). |
| Forever? | No. Surmount will **build its own search product later**. Until then do not add Meilisearch/ES "for convenience." |

### RocksDB knobs (host-agnostic starting points)

Upstream defaults (Stalwart RocksDB docs):

| Knob | Upstream default | Meaning |
|------|------------------|---------|
| `blobSize` | 16834 | Min bytes to treat as blob vs inline metadata |
| `bufferSize` | 134217728 (128 MiB) | In-memory write buffer |
| `poolWorkers` | number of logical CPUs | DB worker threads |

Default `poolWorkers` follows **logical CPU count on the machine the operator
chose**. Leave that default unless oversubscription is measured. Do not pin a
public core count in docs.

**Recommended starting points (reasoning, not eternal law; re-measure on host):**

| Knob | Start with | Why |
|------|------------|-----|
| `blobSize` | **omit (16834)** or keep default | No evidence yet on Surmount attachment mix. Default is fine until import metrics say otherwise. Changing later is a store/settings apply concern; do not bikeshed Day-1. |
| `bufferSize` | **omit (128 MiB)** initially; optional raise to **256 MiB** (`268435456`) if write-heavy import stalls and **measured** RAM headroom is clear | Upstream 128 MiB is a small write buffer vs whole-node RAM on a typical mail box. Do not jump to multi-GiB buffers without measurement; Stalwart has other caches too. |
| `poolWorkers` | **omit (defaults to logical CPUs)** | Leave at default. Cap later only if oversubscription is measured. |

Module today: `services.stalwart.blobSize` / `bufferSize` in
`modules/stalwart-service.nix` (null = omit from `config.json`).
`poolWorkers` is not yet a first-class Nix option; add only if we later choose
to pin it, or set via Stalwart apply/WebUI Day-2.

**Binary FOD note:** current package is a release binary that embeds its own
RocksDB. Prefer `pkgs.rocksdb` (or future fixpkgs rocksdb) when we
source-build under Nix. See [principles.md](principles.md) section 4 and
[packages-and-forks.md](packages-and-forks.md).

---

## 3. Packages and forks

| Item | Direction |
|------|-----------|
| Stalwart source when we need patches | **SurmountSystems/stalwart** fork so Surmount can patch when needed |
| Flake integration | Fork **must** be consumed as a **Nix flake input** (URL + rev the operator bumps), not a forever side manual build outside the flake |
| Agents and git | Agents **never** touch git on the fork unless explicitly instructed. No agent `commit` / `push`. |
| Without full FixOS / fixpkgs yet | **Surmount package overlay** in this repo over **upstream nixpkgs**; path to a **future fixpkgs channel** later. Plain English in packages doc. |

Detail: [packages-and-forks.md](packages-and-forks.md).

---

## 4. Edge (hard)

| Item | Direction |
|------|-----------|
| nginx | **No nginx** as product edge. Security concern restated. |
| In tree today | nginx in `modules/web.nix` is **transitional-to-delete**. Keep working until cutover; do not expand nginx surface. |
| Prefer | **First-party Axum** (or Axum + Leptos stack) as the **HTTPS edge**, not a separate reverse-proxy product (Caddy / nginx / Sozu) unless measured need appears |
| Shape | Edge work is still real (TLS, certs, rate limits, routing to UI vs Stalwart loopback, headers) but can live **in-process** or as a **small `surmount-edge` crate in the same workspace** driven by Axum / tower / rustls. Not a third-party proxy daemon by default. |
| Local hops | Prefer **Unix domain sockets**, not TCP localhost |
| If TCP local | Low nonstandard port, firewalled |
| Public ports | Standard ports only via first-class binding (80/443 edge; mail ports on Stalwart) |
| Caddy / Sozu / etc. | Optional later if measured need; **not** the default product identity |

### TLS posture (operator wants)

| Item | Direction |
|------|-----------|
| HTTP :80 | Automatically and gracefully upgrade/redirect to :443 |
| Insecure TLS | Disable SSLv3, TLS 1.0, TLS 1.1; **prefer TLS 1.3** |
| PQ on TLS | Enable PQ / hybrid cipher suites (KEX) where the stack supports (rustls aws-lc-rs hybrid groups today) |
| Public CA | Open to better CA than Let's Encrypt if automated and not too pricey (ZeroSSL, Buypass, Google Trust Services, commercial ACME, etc.). Not locked. |
| Third-party proxy | No product dependence on nginx/Caddy/etc. as the edge |

### PQC / PQConnect

| Item | Direction |
|------|-----------|
| E2EE PQC | **First-class priority** |
| TLS hybrid PQ KEX | **First-class** on Axum/rustls (aws-lc-rs groups such as X25519MLKEM768) |
| PQConnect | Evaluate and plan for https://github.com/jedisct1/pqconnect (path-layer PQ between supporting peers). **Separate path** from TLS hybrid KEX; does not replace Axum HTTPS or mail TLS alone. |
| Packaging | PQConnect not in nixpkgs as of 2026-07-30 research; Surmount overlay if adopted |
| Cloudflare | **Research appreciation only.** Cite CF Research PQ TLS / hybrid / migration posts for learning. **Do not** use Cloudflare products in the stack (no orange-cloud required path, Workers, Tunnel, CF WAF, CF Access). |

Research: [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md)
(includes plain URLs to CF Research posts).

### Merciless access control

| Item | Direction |
|------|-----------|
| Rate limit | IP rate limiting at edge (tower-governor or equivalent) |
| Ban | Any unauthorized access of any kind -> **immediate blacklist** on the box (precise signal classes still open; see research) |
| Whitelist | Explicit whitelist IPs: never blacklisted; may bypass blacklist |
| Hygiene | Track **last-used** on whitelist entries |
| Implementation lean | Prefer nftables + custom Rust long-term; fail2ban is Python transitional sketch only |

Research: [research/access-control-fail2ban.md](research/access-control-fail2ban.md).
Module pointer: `modules/hardening.nix` header.

Honest scope: "edge" is not free. It is still TLS termination, ACME or another
cert path, rate limits, vhost routing, and security headers. Operator preference
is to own that in **our Axum stack** rather than run a separate proxy product.

Candidates, cutover, and TLS trust research:
[EDGE_AND_TLS.md](EDGE_AND_TLS.md),
[research/rust-edge-and-uds.md](research/rust-edge-and-uds.md),
[research/tls-trust-and-acme.md](research/tls-trust-and-acme.md),
[research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md),
[research/access-control-fail2ban.md](research/access-control-fail2ban.md).

---

## 5. Identity and secrets

| Item | Direction |
|------|-----------|
| Product user/operator auth | **Nostr keys**; dead simple |
| Key management | Host OS via other Surmount tools; **not** manual password management for users |
| Mail credentials | Internal to mail engine; tracked/integrated for humans via **Vaultwarden** (https://github.com/dani-garcia/vaultwarden/) |
| Deploy secrets | Secrets that must exist on disk **before/during** `nixos-rebuild switch` so systemd can start services. **sops-nix** is the current scaffold tool; the *need* is real; the *tool* can change. Material is **host-local / out-of-band**, never in public git. |
| **NEVER secrets in git** | Absolute for this public repo. No plaintext, no ciphertext of production secrets, no age private keys, no LUKS unlock material. Do not suggest "encrypted in git" patterns. [hygiene.md](hygiene.md). |
| Disk | **LUKS2 FDE** first-class consideration for the VPS (disk encryption; not the same as deploy secrets or Vaultwarden). Unlock: passphrase / initrd SSH / TPM. |
| sops vs LUKS | **sops-nix does not unlock or configure LUKS2.** Activation decrypt is after root is up. Never put LUKS unlock material in git. See [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md). |
| Stack language | Prefer **NixOS + Nix + Rust**. **No Python** product/ops (fail2ban transitional). **No NPM**. Gaps filled in-house. |
| Secrets manager wording | **Bitwarden Secrets Manager** is a separate cloud SM product/API (`bws`, SM SDK). **Vaultwarden** implements the password manager client API and does **not** currently expose Secrets Manager APIs (maintainer: licensed feature; SM API gap on VW). Machine startup secrets stay deploy secrets; human/ops inventory in VW; optional `bw` CLI workflows. See [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md). |
| **Arti hidden services (REQUIRED)** | **Arti** = Tor Project Rust Tor. Surmount Server services **must** be reachable via Arti **onion/hidden services**. First-class product surface **alongside** clearnet (where clearnet applies). **Not** optional anonymity. **Not** a clearnet edge replacement. Still no Cloudflare products; own stack. HS keys and startup material = deploy secrets on host (**never in git**); human inventory (onion notes, recovery) = Vaultwarden. Relay/egress remain separate open choices. Research: [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md). Pin: [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7. |

### Two buckets (plus optional disk encryption)

Do **not** invent a pile of vague "seals." Two product buckets only:

| Bucket | Name | Who / when | Job |
|--------|------|------------|-----|
| **1** | **Deploy secrets** (machine / config) | Nix activation; before/during service start | Decrypt service secrets **on the host** so units can start. Scaffold tool: **sops-nix**. Secrets **not** in public git. |
| **2** | **Vaultwarden** | Humans after the host is up | Human/org vault for credentials people use; mail-related secrets operators manage in the vault product. |

Optional third (different concern):

| | **LUKS2** |
|--|-----------|
| Job | Full disk encryption / offline snapshot protection |
| Not | Not deploy-secret distribution; not a human password manager; unlock material never in git |

| | **Deploy secrets (sops-nix today)** | **Vaultwarden** |
|--|-------------------------------------|-----------------|
| Who | Machines / Nix activation | Humans and teams |
| When | Before/during service start | After the host is up |
| What | restic password, VW admin token, session keys, bootstrap mail secrets | Operator passwords, TOTP, shared notes, mail app-password inventory UX |
| In public git? | **Never** (plain or ciphertext) | Server data on host disk; not the deploy channel |
| Replaces the other? | **No** | **No** |

Operator is right to question "yet another secrets tool." The split is not
tool worship. Nix activation cannot open a human vault over HTTP to decrypt
the secrets that start that vault. Something must make **deploy secrets**
available at NixOS activation (age + sops-nix is the current scaffold;
alternatives can be evaluated without dismissing Vaultwarden). Git is not
the secret store for a public tree.

Full write-up: [SECRETS.md](SECRETS.md). Hygiene top rule: [hygiene.md](hygiene.md).

---

## 6. Product

| Item | Direction |
|------|-----------|
| Web stack | **Axum + Leptos SSR** still preferred |
| Phase order | **Admin then webmail** (yes on prior product yeses) |
| Legacy sites | **Static sites only**, no exceptions |
| Spam / security | First-class **spam detection**, lock down; security and integrity highest priority |
| Search product | Stalwart internal FTS now; **Surmount builds own search later** |
| Single VPS end | When operator says |
| Language/runtime product deps | **Nix + Rust** product stack. **No Python** and **no NPM/Node** as product dependencies. Gaps filled in-house (Rust/Nix; tiny shell scripts OK for operator helpers). |
| Mail legitimacy | **Earn trust** with automatable DNS/TLS auth: SPF, DKIM, DMARC, PTR/rDNS, MTA-STS, TLS-RPT; plan DNSSEC + DANE/TLSA without blocking Day-1 if WebPKI path is green. Cert automation affordable; custom CA options **not locked**. Checklist: [DNS.md](DNS.md). |

### DKIM dual-sign and quantum honesty (operator direction 2026-08-11)

| Item | Direction |
|------|-----------|
| Classical Day-1 DKIM | **Dual-sign:** Ed25519 (selector `stalwart`) **and** RSA-4096 (selector `stalwart-rsa`). Both public keys in DNS; both `DkimSignature` objects when engine token works. |
| RSA size | **4096** floor for receiver compatibility hardness. RSA-2048 is not enough. **Do not** use RSA-8192 for DKIM (RFC 8301 verifier MUST range through 4096; larger often breaks). |
| Post-quantum | **Neither** Ed25519 nor RSA-4096 is post-quantum. Larger RSA does **not** buy meaningful quantum resistance (Shor). Do not claim PQ from RSA-4096. |
| Future PQ mail auth | Residual: when IETF and receivers support PQ or hybrid mail auth, Surmount tracks it. Separate from D1 TLS hybrid KEX and D2 PQConnect. |
| Living docs | [DNS.md](DNS.md) *Hardness and quantum honesty*, [SECURITY.md](SECURITY.md), [OPS.md](OPS.md), [SECRETS.md](SECRETS.md), RESIDUAL B6 |

---

## 7. Process and language

| Item | Direction |
|------|-----------|
| Versions | Always validate pins are still latest when touching them |
| Hermeticity | Idiomatic Nix; locked flake; FODs for fixed fetches |
| Native libs | Prefer system shared libraries with fixed ABI where better than crate-bundled (RocksDB design goal on source build) |
| Naming | Facta Non Verba / Fix / FixOS as already documented |
| Glossary | Required: FOD, Day-1, Day-2, apply plans, fixpkgs, etc. |
| Questions | Global **Q1, Q2, ...** in a file; never collide with section numbers |
| No ADR jargon | Plain American English only |
| Secrets language | Prefer **encrypted deploy secrets** / **secrets available at NixOS activation**. Avoid vague "seal" jargon. |
| Docs created/updated this dump | principles, glossary, operator-direction, packages-and-forks, plus living doc pointers |

---

## 8. What this supersedes in older docs

| Older claim | Now |
|-------------|-----|
| Invented default hosting brand or plan SKU | Operator-chosen VPS; size/plan open (Q-HOST-1); no invented names |
| "Caddy is the target edge default" | **Axum-first** HTTPS edge preferred; nginx transitional-to-delete; separate proxy products only if measured need |
| All-RocksDB only as fragile scaffold with heavy "open" pressure | **RocksDB fine for now**; co-location OK this phase |
| External FTS as near-term open pressure | Internal FTS **good enough for now**; own search product later |
| RPO/RTO as near-term backup design work | **Later, not now** |
| architecture-review sizing/host rows that invent host SKUs | Superseded: size open (Q-HOST-1); RocksDB OK-for-now; see banner on that file |
| "Seal" as primary secrets vocabulary | Prefer **deploy secrets** / activation wording (see SECRETS.md) |

---

## 9. Follow-up answers (2026-07-30)

Answers to the open questions that were listed earlier the same day. Living
docs updated to match.

### Q1. Provider / LUKS

| | |
|--|--|
| **Answer** | Operator will ask the VPS provider about **NixOS + LUKS2** recommendations. **Assume** the provider allows NixOS. **Do not invent a provider name** in docs. |
| **Still open** | Exact provider/SKU; whether first install is LUKS2 from day one or interim plain disk then reinstall (depends on provider path). |
| **Working ids** | See [open-choices.md](open-choices.md) **Q-HOST-1**, **Q-HOST-2**. |

### Q2. Stalwart fork

| | |
|--|--|
| **Answer** | Use **SurmountSystems/stalwart** so Surmount can patch when needed. Integrate as a **flake input**, not a side manual build forever. Agents still never git commit/push the fork unless explicitly instructed. |
| **Status** | Directed. Detail: [packages-and-forks.md](packages-and-forks.md). |

### Q3. Secrets language ("seal") and buckets

| | |
|--|--|
| **Answer** | Operator found "seal" unclear. What was meant: some secrets must exist on disk **before/during** `nixos-rebuild switch` so systemd can start services. That is **deploy secrets**, separate from Vaultwarden. Document **two buckets** only (deploy secrets + Vaultwarden). LUKS is optional third and is **disk encryption**, not either bucket. |
| **Status** | Directed language + split. Tool for deploy secrets remains scaffold **sops-nix** (need real; tool can change). |
| **Hygiene correction (same day)** | **Never secrets in git** for this public repo (plain or ciphertext), including no sops-encrypted LUKS keyfiles. Host/out-of-band only. |

### Q4. Edge / proxy

| | |
|--|--|
| **Answer** | Prefer implementing the HTTPS edge in **Axum** (or Axum + Leptos stack / small same-workspace edge crate) rather than a separate reverse-proxy product, unless measured need appears. nginx remains transitional-to-delete. |
| **Status** | Directed preference. Edge work is still real scope; see EDGE_AND_TLS.md. |

### Q5. ACME / centralized CA

| | |
|--|--|
| **Answer** | Want deeper security and performance thought: centralized CAs in a supply-chain era, AI-era vulns, weak disclosure culture. **Do not** lock ACME-only or rustls-acme-only this turn. |
| **Status** | Research open. See [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md). Remaining ids **Q-TLS-1** ... |

### Q6. poolWorkers / cores

| | |
|--|--|
| **Answer** | Leave `poolWorkers` at upstream default (**number of logical CPUs** on the host the operator chose). Cap only if oversubscription is measured. Do **not** invent or publish a core count as product law. |
| **Status** | Directed: leave default; host size remains open (Q-HOST-1). |

---

## 10. Still open (unique ids)

These are not answered yet. Full list and context: [open-choices.md](open-choices.md).

| Id | Topic |
|----|-------|
| **Q-HOST-1** | Exact VPS provider and plan SKU (do not invent names) |
| **Q-HOST-2** | LUKS2 from day one vs interim plain disk (after provider answers) |
| **Q-LUKS-1** ... | Unlock pattern (human initrd SSH vs weaker host-local keyfile-on-boot); unlock material never in git; see luks2 research |
| **Q-TLS-1** ... | Cert trust path for browser HTTPS vs mail TLS (research; no forced choice) |
| **Q-CA-1** / **Q-CA-2** | Primary public CA; multi-CA failover worth it on one VPS? |
| **Q-PQC-1** ... | PQConnect Day-1 vs Day-2; client audience; mail PQ path |
| **Q-ARTI-2** ... | Arti HS **required**; open: which surfaces, admin/JMAP onion, relay ToS (**Q-ARTI-1** answered: required) |
| **Q-SEC-SM-1** ... | True Secrets Manager API need vs deploy secrets + VW PM |
| **Q-ACL-1** ... | Unauthorized definition; whitelist store; ban duration; Rust guard timing |
| **Q-EDGE-1** | Shared cert files vs in-process issuance for web vs mail split |
| **Q-EDGE-2** | UDS path layout under `/run/surmount/` vs per-service |
| **Q-AUTH-1** | Session store shape, first-operator bootstrap allowlist, key-loss recovery |
| **Q-DEP-1** | Keep sops-nix long-term vs evaluate a different deploy-secrets tool (need stays) |

---

## 11. Follow-up (2026-08-09): operator workstation OS secret store

**Status:** operator direction **2026-08-09** (chat). Working product direction
for custody layout. Offline bridge + S4 deploy glue + S7a VW module ship in
tree; **not** full product acceptance of a libsecret Rust crate, live keyring
CI, or host-enabled Vaultwarden / live requireDeployMaterial.

| Item | Direction |
|------|-----------|
| Project tree | Secrets **never** in the project git tree (plain or ciphertext). Unchanged absolute. |
| Operator-side custody | Prefer an **OS-level secret store** on the **operator workstation** (laptop/admin machine). Candidate: **GNOME Secret Service** (libsecret / gnome-keyring). Get far without inventing a full Secrets Manager first. If something perfect does not exist, **build it later**. |
| Host at activation | Server is (or will be) deployed. Host still needs **activation material** as files / systemd credentials / sops-nix (or Q-DEP-1 alternative) on the box (e.g. `/run/surmount-secrets/...`). Product today: env + file paths only. |
| Human vault | Vaultwarden for day-to-day passwords/TOTP (S7a module offline; host enable residual); never feeds nixos-rebuild at activation. |
| Bridge | Laptop store does **not** replace host files. Install bridge (`secrets-install-host.sh`) + opt-in deploy glue (`deploy-host --install-secrets`) shipped offline; live material still operator host work. |
| Intent | Intentional secrets storage **before** more secret material sprawl. |

Three domains (A workstation / B host deploy / C human vault): full write-up
[SECRETS.md](SECRETS.md) section 1. Compaction: [COMPACTION-PIN.md](COMPACTION-PIN.md)
section 8. Residual: [../RESIDUAL.md](../RESIDUAL.md).

Do **not** invent new Q-DEP / Q-SEC answers from this pin. Offline bridge
(`secrets-install-host.sh`) and opt-in deploy glue (`deploy-host
--install-secrets`) are already in tree. Open remains: live host material +
`requireDeployMaterial` (S6), Vaultwarden host enable (S7b), Q-DEP-1 tool
choice, Q-SEC-SM-*.

---

## 12. Follow-up (2026-08-20): mail-domain DNSSEC same class

**Status:** operator direction **2026-08-20** (chat). Working product
direction. Not a lock of Namecheap internals beyond what they already
run.

The operator wants **both** mail domains this host sends or receives
through at the **same security and deliverability class** as
`surmount.systems`, **including DNSSEC**. They were **not** asking to
leave `cryptoquick.com` unsigned.

| Item | Direction |
|------|-----------|
| Class | Extra mailbox domains (`cryptoquick.com`, later mailbox domains) get the same mail records **and** the same hosted DNSSEC as the primary. Not leftover-unsigned. Static-site-only extra vhosts are still **not** automatically mail domains. |
| Algorithm | **ECDSA P-256 SHA-256** (algorithm 13) is acceptable for now. Prefer a matching parent DS with digest type 2 (SHA-256). Digest type 1 (SHA-1) is a fail even if a DNSKEY exists. |
| `surmount.systems` | Operator already toggled Namecheap Advanced DNS **DNSSEC Status ON**. Live public lookups may still be waiting for parent DS plus apex DNSKEY. That wait is Namecheap publish, not a second product decision. |
| `cryptoquick.com` | Operator toggled **OFF** only because agents asked after a leftover parent DS (key tag 2368, algorithm 13, digest type 1 SHA-1, no matching DNSKEY) SERVFAILed validating resolvers. That was sequencing. They now want **ON**, same as `surmount.systems`. Do **not** re-add 2368 by hand. |
| Sequencing vs unsigned | A leftover unmatched parent DS is a SERVFAIL **bug to clear, then sign**. It is **not** "cryptoquick stays unsigned." Enabling hosted ON while stale SHA-1 DS remains can stay SERVFAIL until that DS drops. |
| HTTPS leaf | Do **not** add `cryptoquick.com` / `www` to the production Let's Encrypt leaf until validating lookups succeed. Not this turn while SERVFAIL. |
| DMARC | Live mailbox policy is **`p=quarantine`** (follow-up same day). Do **not** use `p=reject`. |

Living maps: [DNS.md](DNS.md) *Best DNSSEC we can actually run*,
[COMPACTION-PIN.md](COMPACTION-PIN.md), [../AGENTS.md](../AGENTS.md),
[../RESIDUAL.md](../RESIDUAL.md).

---

## 13. Follow-up (2026-08-20): `_dmarc` p=quarantine

**Status:** operator direction **2026-08-20** (chat: quarantine is fine).
Working product direction.

Set `_dmarc` to **`p=quarantine`** on every domain this host sends or
receives mail for. Do **not** use `p=reject`.

| Domain | Direction |
|--------|-----------|
| `surmount.systems` | Change live `_dmarc` from `p=none` to `p=quarantine`. Keep existing rua (`admin@surmount.systems`) and `pct=100`. |
| `cryptoquick.com` | Already `p=quarantine`. Leave as-is. |
| `baxterartworks.com` | Mail plus static. `_dmarc` **intended** is **`p=quarantine`** (rua `admin@baxterartworks.com`). Do **not** restore getHosts `_dmarc` to `p=none`. Public `_dmarc.baxterartworks.com` may stay NXDOMAIN while Namecheap EmailType is FWD even when getHosts has the TXT. |
| Static-only extra vhosts | Do **not** invent `_dmarc` on `yiffa.app`, `btcfur.com`, `nostrfurs.com`, `exophiles.org`, `iantuckerstudios.com`. |

Living maps: [DNS.md](DNS.md), [../AGENTS.md](../AGENTS.md),
[../RESIDUAL.md](../RESIDUAL.md).

---

## Related updates

Living docs updated to match this direction (same turn as this file and the
Q1-Q6 follow-up): STACK, EDGE_AND_TLS, SECRETS, open-choices, hygiene,
architecture-review banner where needed, README, AGENTS, principles,
glossary, packages-and-forks, research/tls-trust-and-acme.md,
research/rust-edge-and-uds.md top note.

Security/PQC/access dump (same day later): research/luks2-and-deploy-secrets.md,
research/pqconnect-and-pqc.md, research/access-control-fail2ban.md; living
EDGE_AND_TLS, SECRETS, SECURITY, principles, glossary, hardening.nix headers;
join `.grok/joins/security-pqc-access.md`.

Arti / PQ research cite / mail earn-trust / no-Python-NPM dump (same day):
research/arti-and-secrets-manager.md; pqconnect-and-pqc CF Research section;
DNS.md earn-trust checklist; principles 5b + hygiene 2b; light STACK /
open-choices / SECRETS / EDGE updates; join `.grok/joins/arti-pqc-mail-docs.md`.
**Correction same day:** Arti onion/HS is **REQUIRED** (not optional). Living
docs + [COMPACTION-PIN.md](COMPACTION-PIN.md); join
`.grok/joins/arti-required-compaction-pin.md`.

Hygiene correction (same day): never secrets in git (absolute); LUKS research
scrubbed of git-keyfile suggestions; stack language NixOS+Nix+Rust / no
Python product-ops / no NPM. Join `.grok/joins/hygiene-no-git-secrets.md`.
