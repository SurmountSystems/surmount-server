# Surmount Server engineering principles

Standing principles for how this repo is built. Product direction that the
operator stated on a date lives in [operator-direction.md](operator-direction.md).
Open items live in [open-choices.md](open-choices.md). Terms live in
[glossary.md](glossary.md).

**Last updated:** 2026-07-30

Nothing here invents eternal law beyond what the operator has directed. When
operator direction and an older scaffold conflict, **operator direction wins**
and the scaffold is marked transitional.

---

## 0. NEVER secrets in git (absolute; public repo)

- Public tree contains **zero** secret material: no plaintext, no ciphertext
  of production secrets, no age/sops private keys, no LUKS unlock material.
- Do **not** suggest committing secrets "because encrypted."
- Secrets live on the host / operator-controlled channels only.
- Standing checklist: [hygiene.md](hygiene.md) top rule; [SECRETS.md](SECRETS.md).

## 1. Hermetic flakes

- Builds and evals come from a locked `flake.lock`.
- No impure network at eval or build except properly hashed fixed-output
  derivations (FODs). See [glossary.md](glossary.md) for FOD.
- Secrets decrypt **on the host** at activation. They are not in the public
  git tree (plain or ciphertext) and not part of pure evaluation.
- `ref/` is study-only. The flake must evaluate if `ref/` is empty.
- Prefer idiomatic Nix: small modules, overlays for Surmount packages, clear
  package paths under `nix/packages/`.

## 2. Prove assumptions; do not scare-copy

- Prefer measured fact: green/red build, failed import, restore drill, load
  number.
- Do not claim "unsafe," "dangerous version skew," or "must not" without
  evidence.
- Docs can lag or lie. Code, tests, and packaging joins win for versions and
  ports. Update living docs in the same turn as design changes.

## 3. Prefer current major versions

- Greenfield prefers **current** majors of engines we choose (mail, edge, UI
  deps).
- **nixpkgs lag is not a reason to stay old.** Surmount package overlays,
  overrides, or vendored FODs are fine when upstream is ahead of the channel.
- Compatibility with old engine majors is **not** a default goal unless the
  operator says so.
- Always re-check that pinned versions are still current when bumping.

## 4. System shared libraries with fixed ABI (when better)

When we **source-build** C/C++-backed crates under Nix:

- Prefer linking a known system package (for example `pkgs.rocksdb` or a future
  fixpkgs rocksdb) with a pinned version and auditable ABI.
- Prefer that over an opaque crates.io `librocksdb-sys` tree that vendors its
  own RocksDB copy, when the product build allows it.
- Reason: one RocksDB to audit, one set of security patches, deterministic
  link against the same library the rest of the host can share.
- **Today's gap:** Stalwart is packaged as a **release binary FOD**. That
  binary already embeds its own RocksDB. System-lib linking is a **design goal
  for the source-build path**, not a claim about the current binary. Document
  the gap; do not pretend the FOD is dynamically linked to `pkgs.rocksdb`.

Same spirit applies to other native deps (OpenSSL, zlib, etc.) where Nix
already patches or wraps correctly.

## 5. Packages without full Fix / fixpkgs yet

Plain names (no ladder jargon required to read this):

| Name | Meaning |
|------|---------|
| **upstream nixpkgs** | Community package set and modules we consume via the flake input |
| **Surmount package overlay** | Packages and modules we own in *this* repo (`nix/packages/`, `modules/`) so we are not stuck on channel lag |
| **future fixpkgs channel** | Later shared Surmount package channel/set, if multiple repos need the same pins |

Today: own critical packages here (Stalwart pin, management UI, related FODs).
Later: graduate repeated pins into a fixpkgs-style channel if needed. Full
Fix / FixOS ladder: [fix-and-fixos.md](fix-and-fixos.md). Packaging consume
pattern: [packages-and-forks.md](packages-and-forks.md).

## 5b. Stack language: NixOS + Nix + Rust (no Python, no NPM)

Prefer **NixOS + Nix + Rust** for product and ops gaps. **Gaps are filled
in-house** (Rust crates, Nix modules, tiny POSIX shell under `scripts/`), not
by pulling a Python or NPM ecosystem "for speed."

| Avoid in product/ops stack | Prefer |
|----------------------------|--------|
| **Python** product deps (fail2ban is transitional-at-most) | Rust services + nftables / NixOS modules |
| **NPM** / Node package ecosystem / `package.json` product trees | Rust (Axum, Leptos, tower) and Nix packaging |
| Python/Node control planes for edge, UI, or mail | In-house Rust + Nix |

Short why:

- **Supply chain:** fewer dynamic language package graphs; Nix pins inputs;
  Rust crates lock with cargo + crane under the flake.
- **Portability:** one host story (NixOS modules + static-ish Rust bins), not
  a second runtime and site-packages tree.
- **GC / ops:** no always-on interpreter control plane; replace transitional
  Python (fail2ban) rather than grow it.
- **Correctness:** typechecked Rust at the edge and UI; Nix for config and
  closure purity.

Leptos/Rust web stays on the Rust toolchain (not a React/Vue NPM SPA).
**Arti** (required onion/HS path) and **PQConnect** (separate PQ path when
adopted) still land as Nix packages / Rust where upstream allows; do not grow
a parallel Python/NPM control plane around them. (Upstream PQConnect itself is
Python; Surmount packaging is Nix-owned and product control plane stays
Nix+Rust.)

## 6. Security first

- First-class **spam detection**, lock-down, and integrity. Mail is a hostile
  protocol surface.
- Minimize attack surface: few public ports, hardened SSH, abuse controls,
  no required third-party WAF as a crutch.
- **Merciless access control** (operator direction 2026-07-30): IP rate
  limits at the edge; unauthorized access -> fast host blacklist; explicit
  **whitelist** never banned; track whitelist **last-used**. fail2ban is a
  transitional sketch; long-term prefer Rust + nftables. Design:
  [research/access-control-fail2ban.md](research/access-control-fail2ban.md).
- **No nginx** as the product edge (operator direction 2026-07-30). Current
  nginx in tree is **transitional-to-delete**. Prefer a **first-party Axum**
  HTTPS edge (in-process or small same-workspace crate) with TLS, cert
  automation or shared cert files, and rate limits. Separate reverse-proxy
  products only if measured need. See [EDGE_AND_TLS.md](EDGE_AND_TLS.md).
  Cert trust path is **not** locked to ACME-only:
  [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md).
- TLS posture lean: :80 redirects to :443; no SSLv3/TLS1.0/1.1; prefer TLS
  1.3; enable hybrid **PQ KEX** as a **first-class** rustls/aws-lc-rs edge
  feature; open to automated public CAs beyond LE (not locked). **E2EE PQC**
  (including PQConnect) is a **separate** first-class research priority:
  [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md). Cloudflare
  Research PQ blog posts may be cited for learning; **no** CF products in stack.
- Prefer **Unix domain sockets** between local services over TCP localhost.
  If TCP is required: low nonstandard port, firewalled; standard ports only
  via first-class public binding.
- **Arti onion/hidden services are REQUIRED** product surface alongside
  clearnet (Tor Project Rust Tor). Not optional. Not a clearnet edge
  replacement. HS keys never in git. See
  [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md),
  [COMPACTION-PIN.md](COMPACTION-PIN.md).
- Product auth: **Nostr keys** (simple; host OS / other Surmount tools manage
  keys). Mail credentials stay in the mail engine and are tracked for humans
  via **Vaultwarden**. **Deploy secrets** must be available at NixOS
  activation (sops-nix scaffold today; need is real; tool can change;
  material **not** in public git). See [SECRETS.md](SECRETS.md).
- **LUKS2 FDE** is a first-class host consideration for the VPS (disk
  encryption; not the same as deploy secrets or Vaultwarden). Unlock:
  passphrase / initrd SSH / TPM. **sops-nix does not unlock LUKS2.** Never
  put unlock material in git.
  [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

## 7. Local IPC: sockets before localhost TCP

```text
  Prefer:  edge  --UDS-->  management-ui
           edge  --UDS-->  stalwart HTTP (if proxied)
           ui    --UDS-->  vaultwarden (when added)

  Avoid by default:  127.0.0.1:random for every hop
  If TCP local:      nonstandard port + firewall deny from non-local
  Public:            25/465/587/993/4190 mail; 80/443 edge only
```

## 8. Single operator-chosen VPS (for now)

- Host is an **operator-chosen VPS** (not a default Hetzner preference). Do
  not invent a provider name or publish assumed RAM/disk/core/SKU numbers
  (**Q-HOST-1**). Assume NixOS is allowed; operator will confirm NixOS + LUKS2
  with the provider.
- Minimize idle load; use cores when useful (FTS, compaction, builds).
  `poolWorkers` defaults to logical CPU count on whatever machine the
  operator chose (leave default unless oversubscription is measured).
- Single VPS ends **when the operator says**. Do not invent scale-out triggers
  or mention other projects as exit criteria.

## 9. Product shape (direction)

- **Axum + Leptos SSR** preferred for the Surmount web product.
- **Admin console first**, then **real webmail** in v1.
- Legacy sites: **static files only**, no exceptions for old Synology apps.
- Stalwart owns mail protocols, message store, spam path, and **internal FTS
  for now**. Surmount will build its **own search product later**; internal
  FTS is good enough until then.
- RocksDB for all store roles is **fine for now** (all-role co-location OK
  this phase). No blob tiering or compliance split now. Clever RPO/RTO backup
  design is **later, not now**.

## 10. Agents and git

- Agents **never** `git commit` or `git push` (human-signed only).
- Agents **never** commit or suggest committing secrets (plain or
  ciphertext). See section 0 and [hygiene.md](hygiene.md).
- Stalwart patches: **SurmountSystems/stalwart**, consumed as a **flake
  input** (not a forever side manual build). Agents do not touch the fork
  repo unless the operator explicitly instructs. See
  [packages-and-forks.md](packages-and-forks.md).
- No bulk find-and-replace across the tree.
- ASCII only in docs we own. No em dashes. No "ADR" jargon. Prefer
  **deploy secrets** language over vague "seal."

## 11. Doc hierarchy

| Doc | Role |
|-----|------|
| [COMPACTION-PIN.md](COMPACTION-PIN.md) | Single reload file after compaction |
| [operator-direction.md](operator-direction.md) | Dated operator direction dump |
| [open-choices.md](open-choices.md) | Still open / proposed |
| [glossary.md](glossary.md) | Terms (FOD, Day-1, Day-2, Fix, Arti, ...) |
| [STACK.md](STACK.md) | Living stack map |
| [packages-and-forks.md](packages-and-forks.md) | How packages and forks are consumed |
| [architecture-review.md](architecture-review.md) | Peer review of tree facts (partially superseded where noted) |
| `docs/research/` | Deep evidence; not living law |

Open questions use **unique global ids** (**Q1, Q2, ...** per file, or
namespaced **Q-HOST-1**, **Q-TLS-1**, etc.). Never collide with section
numbers.

---

## Related

- [hygiene.md](hygiene.md) - operational rules checklist
- [AGENTS.md](../AGENTS.md) - agent standing law
- [fix-and-fixos.md](fix-and-fixos.md) - Fix / FixOS ladder
