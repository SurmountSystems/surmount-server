# Agentic and operational hygiene

Rules for how this repository is built and maintained. Follow them so the
mail host stays reproducible, recoverable, and boring under pressure.

Agent process pins also live in [../AGENTS.md](../AGENTS.md).
Principles: [principles.md](principles.md). Glossary: [glossary.md](glossary.md).
Operator direction: [operator-direction.md](operator-direction.md).

---

## NEVER secrets in git (absolute; public repo)

**Standing law. Operator direction 2026-07-30 (hygiene correction).**

This repository is treated as **public-domain / public**. The public tree must
contain **zero secret material**.

| Forbidden in git (any form) | Examples |
|-----------------------------|----------|
| Plaintext secrets | passwords, tokens, private keys, `.env` |
| Encrypted unlock material | sops/age ciphertext of LUKS keyfiles, recovery keys meant to unlock disk |
| Age / sops private keys | `keys.txt`, host age identities, admin age secret keys |
| Anything that unlocks production | LUKS passphrases, Tang credentials, TPM seed backups if they unlock this host |

**Do not** commit, stage, suggest, or casually brainstorm putting secrets in
git "because they are encrypted." Ciphertext of unlock material in a public
repo is **entirely out of the question**.

**Correct mental model:**

- Secrets stay **off git**.
- Deploy secrets live on the **host** via operator-controlled channels
  (out-of-band copy, local-only age keys that are never pushed, install-time
  upload, private operator machine). Detail: [SECRETS.md](SECRETS.md).
- LUKS unlock is **passphrase**, **initrd SSH**, and/or **TPM** on hardware
  that supports it. Not a keyfile (or sops blob of a keyfile) in this repo.
  Research: [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).
- Agents **never** invent "put the encrypted keyfile in git" as a design option.

If a scaffold or older doc still shows `secrets/*.yaml` as something you
commit: treat that as **transitional bad pattern to delete**. Prefer host-local
paths and operator-managed secret stores outside the public tree.

### Pre-commit private-data scan

Mechanical gate (patterns only) so accidental staging of private material fails
before a commit object exists:

| Piece | Path |
|-------|------|
| Shared scanner | `surmount-private-data` crate (`--staged` / `--tree` / `--paths`) |
| Nix package | `nix/packages/surmount-private-data.nix` (crane, same toolchain as management-ui) |
| Project pre-commit | `script/git-hooks/pre-commit` (host `~/.git-hooks` chains here) |
| Detector fixtures | crate `testdata/` (synthetic only; excluded from tree scan). Leftover-homes **sample** `.sh` files under `crates/surmount-leftover-homes/testdata/` are fixtures, not product drivers. |
| Git hook | `script/git-hooks/pre-commit` is a short shebang that execs the scanner. Git needs that file. It is not a product bash driver. |
| Self-test | crate tests in `crates/surmount-private-data` |

**Classes (regex / path only; never real values in the tool):** PEM / OpenSSH
private key blocks; `AGE-SECRET-KEY-1...`; common token shapes (`ghp_`,
`github_pat_`, `AKIA...`, etc.); password/secret/token assignment heuristic;
secret-like basenames (`.env`, `*.pem`, `id_ed25519`, `keys.txt`,
`secrets.yaml`, ...); long SSH public keys under `hosts/` / `secrets/`;
public-looking IPv4 under `hosts/**` except allowlisted private/docs ranges
(loopback, RFC1918, TEST-NET, link-local, `0.0.0.0`).

On hit the scanner prints **class + file path** only (not the secret line).
Fix: unstage / remove material. Paste keys, PEMs, age identities, and real
public host IPs **on the host only**, never into this public git tree.
Provisioned-host UI screenshots must not be copied into the tree, residual,
reports, or hooks.

```bash
just check-private-data -- --staged   # pre-commit twin (alias: just private-data)
just check-private-data -- --tree     # full tracked tree (CI twin)
nix run .#surmount-private-data -- --tree
# crate tests: cd crates && cargo test -p surmount-private-data
# or: nix build .#checks.<system>.surmount-private-data-test
```

`nix run .#surmount-private-data` (alias `just check-private-data` /
`just private-data`). Pre-commit prefers the binary on PATH, then a
`crates/target` build, then `nix run .#surmount-private-data -- --staged`
so a commit TTY does not need a nix shell. GHA runs the flake package
`--tree` and `just ci` includes crate tests. This gate is admission
control only; it does not replace the standing never-secrets law above.

Reasonable leftover exceptions (fixtures, git hook shebang, laptop-renew
units): [packages-and-forks.md](packages-and-forks.md) *Reasonable leftover
exceptions*.

### Host-specific identity stays off the public tree

Same spirit as the private-data scan: **do not commit host-specific hostnames
or flake attrs named after a real box.** The sample path is
`hosts/mail-vps/` with generic flake attr `#mail-vps` and default
`networking.hostName = lib.mkDefault "mail-vps"`. Operators set the real
hostname on the machine (or a local overlay that never lands in this public
repo). First-deploy box names are operator host identity, not living product
path names.

**Host-local overlay contract:** hardware-config, SSH authorized keys, real
networking, real hostname, PEMs, and age/HS material live in a **private**
`host-local/` directory on the host (or private operator path). Never copy
that material into tracked `hosts/` or `secrets/`. Living contract and
placeholder layout: [deploy-host-local.md](deploy-host-local.md). Operator
deploy driver: `nix run .#surmount-deploy-host` / `just deploy-host` (refuses
host-local into public product paths; fails loud if authorized keys are empty
when checking lockout risk).

**Public repo vs operator facts (Kerckhoffs).** This repository is meant to
be public. Process, architecture, and crypto design belong in the tree.
Living operator facts do not: person mailbox addresses, MailPlus
uid-to-person maps, provisioned host names. Those live under global
`~/.agents/surmount-server/` on this machine (see `operator-facts.md`).
Public docs may **point** at that directory. They must not paste the
roster. The scanner is patterns-only and will not catch every address.

**Agent session notes** are local only (gitignored). Write **plans** to
`~/.agents/plans/` (use `~/.agents/plans/surmount-server/` if a filename
would collide) and **reports** to `~/.agents/reports/` on this machine.
Call those handoffs **reports**, never "joins". Do **not** recreate repo
`.agents/plans/`, `.agents/reports/`, or `.agents/joins/` as a live home.
Do **not** create project-root `.grok/` for reports, plans, or scratch.
Leftover repo `.agents/plans` and `.agents/reports` were moved 2026-08-18.
Leftover repo `.grok/joins` reports were moved the same day to
`~/.agents/reports/`. Product scripts and tests must not mkdir those leftover
homes; use `mktemp` / `$TMPDIR`, or host `~/.agents/reports/` for notes.
`.gitignore` already ignores `.agents/`; keep that ignore.
Do not stage or commit agent notes: they are not product docs and
may record private host work. Older commits may still contain some; do
not grow that set.

---

## 0. No assumed architecture acceptance

- Scaffold, research docs, and agent notes are **not** operator acceptance.
- **Operator direction** dated in operator-direction.md is working product
  direction (stronger than scaffold when they conflict), still not eternal law.
- Prefer **proposed** / **scaffold default** / **research finding** /
  **operator direction YYYY-MM-DD** labels.
- **No ADR jargon.** Plain English open choices only:
  [open-choices.md](open-choices.md).
- Evidence for Stalwart stores (current pin 0.16.15):
  `docs/research/stalwart-0.16.15-stores-evidence.md`.
  Historical 0.11.8 only:
  `docs/research/stalwart-stores-evidence-2026-07-30.md`.

## 0b. Greenfield versions; no scare copy without evidence

- Prefer **current** major versions of engines we choose. nixpkgs lag is not a
  reason to stay old. Always re-validate latest when bumping.
- Stalwart is pinned via **Surmount package overlay**
  (`nix/packages/stalwart-mail.nix`); 0.11.8 was a historical scaffold
  accident from an older host channel (pre-26.05).
- Do not claim "unsafe" or "version skew is dangerous" without a failed build,
  broken import, or measured issue.
- On source builds, prefer system shared libraries with fixed ABI (for example
  `pkgs.rocksdb`) over opaque crate-bundled native trees when practical.
  Binary FOD today embeds RocksDB; document the gap.

## 0c. Hierarchical docs and agents

- Living docs under `docs/`; deep notes under `docs/research/`; parent docs
  link to research.
- Multi-file work: hierarchical subagents; write short reports under
  `~/.agents/reports/` on this machine. Call those handoffs **reports**,
  never "joins".
- Open questions: global **Q1, Q2, ...** per file; do not collide with section
  numbers.

## 1. Pure and reproducible

- Evaluate and build from **locked** `flake.lock` inputs.
- No impure network during eval/build except properly hashed fixed-output
  derivations (**FODs** in the Nix sense; see glossary).
- `nix build`, `nixos-rebuild`, and CI should not depend on your laptop's
  ambient `$PWD` hacks, unpinned `fetchTarball`, or live git checkouts of
  dependencies outside the flake.
- Deploy secrets decrypt **on the host** at activation. They are **not** in
  the public git tree (plaintext or ciphertext). See top rule.

## 2. Secrets: two buckets + optional disk encryption

- **Top rule first:** never secrets in git. See section above.
- **Bucket 1 (deploy secrets):** available at NixOS activation (sops-nix +
  age as scaffold tool today). Material lives **on the host** (or private
  operator channels), not in the public repo. Operator may replace the
  *tool*; the *need* for secrets at activation remains.
- **Bucket 2 (humans):** self-hosted **Vaultwarden** (S7a module offline done;
  S7b enable path scripted by A0 host-cutover after token); does **not**
  replace deploy secrets for activation or flake purity. Mail cred inventory UX.
- **Disk (optional third):** **LUKS2** FDE first-class when install allows.
  Unlock: passphrase / initrd SSH / TPM. sops-nix does **not** unlock LUKS2.
  Never put LUKS unlock material in git. Detail:
  [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).
- Never put secrets in `flake.nix`, host modules, or world-readable Nix
  store paths.
- **Do not dual-stack agenix + sops.**
- Prefer **deploy secrets** language over vague "seal."
- Detail: [SECRETS.md](SECRETS.md), [SECURITY.md](SECURITY.md),
  `secrets/README.md`.

## 2b. Stack language: NixOS + Nix + Rust (no Python, no NPM)

- Prefer **NixOS + Nix + Rust** for product and ops gaps. **Gaps filled
  in-house** (Rust, Nix modules, tiny `scripts/` shell).
- **No Python** product dependencies. fail2ban (Python) is
  transitional-at-most and should be replaced (Rust + nft lean).
- **No NPM** / Node product dependencies (`package.json` trees, etc.).
- Canonical detail: [principles.md](principles.md) section 5b.

## 3. `ref/` is study-only (unless operator makes it a locked input)

- Submodules and downloaded references live under `ref/`.
- Implement **our** modules under `modules/` and packages under `nix/`.
- No large unattributed copy-paste from other flakes into production.
- The flake must evaluate if `ref/` is empty.
- Stalwart fork consume: prefer flake input URL+rev; submodule only if
  operator maintains it. [packages-and-forks.md](packages-and-forks.md).

## 4. App logic in Rust; Nix for packaging and config

- Protocol and product logic that we own goes in `crates/` (Rust).
- Nix owns packaging (crane), NixOS modules, deployment, and small
  operator shell helpers.
- Do not grow large Python/bash control planes when Rust + Nix modules
  suffice. Language ban detail: section 2b and principles 5b.
- **Axum + Leptos SSR** is the invested web stack; admin shell is Leptos
  SSR scaffold (ssr-only). Hydrate islands and webmail remain residual
  ([SEARCH_AND_UI.md](SEARCH_AND_UI.md)). Not an NPM SPA.
- Prefer **Unix domain sockets** between local services.

## 5. Migration is operator-run, not silent activation

- One-time Maildir import is a **runbook** (`docs/MIGRATION.md`) or a
  carefully idempotent oneshot the operator starts.
- Never put irreversible data assumptions in `system.activationScripts`
  (no "wipe and reimport", no silent chown of unknown trees).
- Prefer staging under `/var/lib/surmount/import` and explicit CLI.
- Priority: recover Synology MailPlus onto this stack.

## 6. Stalwart owns mail protocols; Rust owns the console and webmail

- SMTP, submission, IMAP, ManageSieve, JMAP, spam filter: **Stalwart**.
- Message store and **internal FTS for now**: **Stalwart** (RocksDB
  co-location **fine for now** per operator direction;
  [DATASTORES.md](DATASTORES.md); short map in [STACK.md](STACK.md)).
- Surmount builds its **own search product later**; do not add external FTS
  for convenience.
- Management console, **v1 webmail**, and product auth: **Rust**
  (`surmount-management-ui`).
- Product auth is **Nostr** (keys via host OS / Surmount tools); Stalwart
  holds mail credentials; human tracking via Vaultwarden.
- Mail search in our UI uses **Stalwart/JMAP first**.
- Stalwart `/admin` (HTTP local only) is a temporary fallback while the
  Rust UI grows. Prefer SSH tunnel; optional edge path only for bootstrap.

## 7. Admin first, then real webmail in v1

- Ship **operator admin** before full webmail; webmail is **in v1 scope**
  after admin.
- Long-term desktop/local-first clients are complementary, not a cancel of
  browser admin or v1 webmail.
- Legacy public sites from Synology are **static files only**; **no
  exceptions**.

## 8. No Cloudflare required path

- Traffic must work **direct to the VPS** (A/AAAA on mail and services).
- Do not require orange-cloud proxy, CF Access, CF Workers, CF WAF, or
  Tunnel as critical path.
- DNS registrar may be anywhere (including Cloudflare DNS-only).
- Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md), [SECURITY.md](SECURITY.md).

## 8b. Arti onion / hidden services (REQUIRED)

- Surmount Server services **must** be reachable via **Arti** (Tor Project
  Rust Tor) **onion/hidden services**. Not optional. First-class next to
  clearnet. Not a clearnet edge replacement. Still no CF products.
- HS keys and Arti startup material: **deploy secrets** on host; **never in
  git**. Human inventory: Vaultwarden (PM API; not Bitwarden Secrets Manager).
- Detail: [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md),
  [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7.

## 9. Edge proxy policy

- Terminate public HTTPS at an edge we control; keep UI and Stalwart HTTP
  local only (UDS preferred).
- **nginx is transitional-to-delete** (works today in `modules/web.nix`).
  Do not expand nginx surface.
- **Target:** **Rust** edge (ACME, rate limit, reverse proxy). Caddy is not
  the preferred identity when Rust is the goal.
  See [EDGE_AND_TLS.md](EDGE_AND_TLS.md).
- Minimize open ports (`modules/networking.nix`).
- Standard ports only via first-class public binding; if local TCP is
  required, nonstandard port and firewalled.

## 10. Doc survival (decisions live on disk)

- Product and architecture notes that must survive chat compaction live under
  `docs/`. **First reload file:** [COMPACTION-PIN.md](COMPACTION-PIN.md).
  Also: operator-direction, open-choices, STACK, DATASTORES, SECURITY,
  SECRETS, principles, glossary, packages-and-forks, this file.
- New durable stores must update the DATASTORES inventory.
- Chat is not the system of record. Update living docs in the same turn
  as the design change.
- Prefer extending these docs over new ephemeral notes.
- Deep / version-pinned notes: `docs/research/`.

## 11. Small modules, comments, checks, tests

- One concern per module under `modules/` with a short header comment.
- Shared knobs live in `surmount.*` (`options.nix`).
- Flake `checks`: package build, config eval, NixOS VM smoke test.
- Prefer expanding tests over undocumented tribal knowledge.

## 12. Security-first, maintainable surface

- Hardening defaults in `hardening.nix` (SSH keys, light fail2ban sketch).
  Target: merciless ban + whitelist + last-used; Rust/nft lean.
  [research/access-control-fail2ban.md](research/access-control-fail2ban.md).
- TLS/:80 redirect, TLS1.3 prefer, PQ hybrid KEX, PQConnect research:
  [EDGE_AND_TLS.md](EDGE_AND_TLS.md),
  [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).
- First-class **spam detection**, lock-down, integrity.
- Backups (`backups.nix`) before clever automation (clever RPO/RTO later).
- Prefer an **operator-chosen VPS** you control (size/plan open; do not invent
  provider names or RAM/disk/core/SKU numbers). Prefer **LUKS2** when
  install path allows.
- Self-ops: persistent size-capped journald (`surmount.logging`), health
  endpoints, `just host-logs` / `just host-logs -- --status` / `scripts/` checks
  ([OPS.md](OPS.md)). Journal files can hold peer IPs and auth-fail
  metadata; they stay on the host, never in git, never on the apex site.
  No required third-party WAF.
- **Single mail VPS** until the operator says otherwise.

## 13. Packages and forks

- Critical engines: **Surmount package overlay** in this repo over upstream
  nixpkgs. Path to **future fixpkgs** later.
  [packages-and-forks.md](packages-and-forks.md).
- Stalwart fork: `SurmountSystems/stalwart` when patches needed. Operator
  bumps flake input rev or maintains submodule. Agents never commit/push the
  fork unless explicitly instructed.
- Agents never `git commit` in this repo (human-signed only).

## Quick anti-patterns

| Don't | Do |
|-------|-----|
| Commit secrets (plain or ciphertext) | Keep secrets off git; host/out-of-band only |
| Suggest sops-encrypted LUKS keyfile in git | Passphrase / initrd SSH / TPM; never unlock material in repo |
| Commit `.env`, age private keys, sops key material | Operator machine and host paths only |
| Expect Vaultwarden to feed nixos-rebuild | Deploy secrets for activation; VW for humans |
| Dual-run agenix + sops | One deploy-secrets system only |
| Grow Python or NPM product/ops surface | NixOS + Nix + Rust |
| `activationScripts` import mail | Operator runs import helper |
| Copy 2k-line flake from the internet | Read it in `ref/`, write small modules |
| Expose Stalwart admin on `:443 /` forever | Local only + SSH tunnel; Rust UI as primary |
| Claim Stalwart speaks Nostr natively | Nostr in Axum; bridge to Stalwart creds |
| Rebuild without reading DNS/migration docs | Follow README checklist |
| Require Cloudflare proxy for mail/HTTPS | Direct A/AAAA to VPS; our edge + Stalwart |
| Parallel full-text index of all mail now | Internal FTS / JMAP first; own search later |
| Leave architecture only in chat | Pin in operator-direction / open-choices / STACK |
| Treat nginx as permanent | Mark delete; Axum-first edge cutover in EDGE_AND_TLS |
| Prefer Caddy because "simple" against Axum-first direction | Axum-first edge; Caddy only emergency bridge |
| Dynamic servers for legacy Synology sites | Static files only, no exceptions |
| Invent multi-node mail triggers | Single VPS until operator says |
| Call notes "ADRs" | Plain open choices |
| Stay on old engine "because nixpkgs" | Prefer current; Surmount package overlay OK |
| Invented default hosting brand or plan SKU | Operator-chosen VPS; size open (Q-HOST-1) |
| Scare about unsafe without evidence | Measure first |
| Agent git commit/push on stalwart fork | Operator-managed consume only |
| Add Python or NPM as product deps | Nix + Rust; fill gaps in-house |
| Claim Vaultwarden is Bitwarden Secrets Manager | VW is PM API; SM is separate (see arti research) |
| Treat CF Research as "use Cloudflare" | Cite research; no CF products in stack |
| Treat Arti onion/HS as optional garnish | **Required** product surface; COMPACTION-PIN section 7 |
| Put Arti HS keys in git (plain or ciphertext) | Deploy secrets on host + offline backup only |
| Ship mail without SPF/DKIM/DMARC/rDNS | Earn trust checklist in DNS.md |
