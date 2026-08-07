# Surmount Server - agent notes

Short process rules for anyone (human or agent) editing this repo.
Product detail lives under `docs/`. This file is standing law only.

## Language

- **No ADR jargon.** Do not write "ADR", "ADR-001", or "architecture decision
  record." Use plain American English: open choices, working notes, proposed,
  scaffold default, research finding, operator direction YYYY-MM-DD.
- **Fix / FixOS naming:** when meaning Surmount-owned Nix or NixOS lineage,
  say **Fix** (Nix take/fork) or **FixOS** (NixOS take/fork). When meaning
  community projects, say **upstream Nix**, **upstream NixOS**, or **nixpkgs**.
  Plain packaging names: **upstream nixpkgs**, **Surmount package overlay**,
  **future fixpkgs channel**. Ladder: [docs/fix-and-fixos.md](docs/fix-and-fixos.md).
  Not operator-accepted product law unless they say so in writing.
- **FOD** means Nix Fixed-Output Derivation only (see
  [docs/glossary.md](docs/glossary.md)). Not "foreign object debris."
- Open questions in docs: global **Q1, Q2, ...** (never collide with section
  numbers).
- Undecided design: [docs/open-choices.md](docs/open-choices.md).
- Dated operator direction: [docs/operator-direction.md](docs/operator-direction.md).
- **After compaction:** [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md) first.
- Living maps: [docs/STACK.md](docs/STACK.md), [docs/hygiene.md](docs/hygiene.md),
  [docs/principles.md](docs/principles.md), [docs/fix-and-fixos.md](docs/fix-and-fixos.md).
- Deep notes: [docs/research/](docs/research/). Parent docs link down to
  research; research does not replace living docs.
- ASCII only in docs we own; no em dashes.

## Greenfield versions

- Prefer **current major versions** of engines we choose (mail, edge, UI deps).
- **Always validate** pins are still latest when touching them.
- **nixpkgs lag is not a reason to stay old.** Surmount package overlay,
  overrides, or vendored FODs are fine when upstream is ahead of the channel.
- Stalwart is pinned in `nix/packages/stalwart-mail.nix` (not channel 0.11.8).
  Compatibility with old Stalwart is **not** a goal.
- On source builds, prefer system libs with fixed ABI (for example
  `pkgs.rocksdb`) over crate-bundled native trees when practical. Binary FOD
  embeds RocksDB today; that gap is documented in packages-and-forks.

## NEVER secrets in git (absolute; public repo)

- **Never** commit secrets: plaintext, ciphertext, age/sops private keys,
  `.env`, LUKS keyfiles or recovery material, or anything that unlocks
  production.
- **Never suggest** committing secrets "because encrypted," including
  sops-encrypted LUKS keyfiles. Entirely out of the question for this
  public tree.
- Secrets stay **off git**. Deploy secrets live on the host via
  operator-controlled channels. LUKS unlock: passphrase / initrd SSH / TPM.
- Detail: [docs/hygiene.md](docs/hygiene.md) (top rule),
  [docs/SECRETS.md](docs/SECRETS.md),
  [docs/research/luks2-and-deploy-secrets.md](docs/research/luks2-and-deploy-secrets.md).

## Stack language (operator direction 2026-07-30)

- Prefer **NixOS + Nix + Rust** for product and ops gaps.
- **No Python** in the product/ops stack (fail2ban Python is
  transitional-at-most; replace with Rust + nft lean).
- **No NPM** ecosystem.
- Short form why: [docs/principles.md](docs/principles.md).

## Edge and security (operator direction 2026-07-30)

- **No nginx** as product edge. In-tree nginx is **transitional-to-delete**.
  Prefer **first-party Axum** HTTPS edge (TLS, certs, rate limit); cert path
  not locked ACME-only. See [docs/EDGE_AND_TLS.md](docs/EDGE_AND_TLS.md).
- Prefer **Unix domain sockets** between local services over TCP localhost.
- **Arti onion/hidden services REQUIRED** (Tor Project Rust Arti); first-class
  next to clearnet; HS keys never in git; not optional. See
  [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md) section 7 and
  [docs/research/arti-and-secrets-manager.md](docs/research/arti-and-secrets-manager.md).
- First-class spam detection, lock-down, integrity.
- Host: **operator-chosen VPS** (not Hetzner-as-default). Size/plan open
  (Q-HOST-1); do **not** invent or publish RAM/disk/core/SKU numbers.
- No Cloudflare products as critical path (research cites OK).

## Evidence before "unsafe"

- Do **not** claim something is unsafe, broken, or "version skew is dangerous"
  without evidence (failed build, broken import, measured issue, failed restore).
- Prefer: measured fact, open question, or historical note.
- Current pin (0.16.15) store evidence:
  [docs/research/stalwart-0.16.15-stores-evidence.md](docs/research/stalwart-0.16.15-stores-evidence.md).
- Historical 0.11.8 only:
  [docs/research/stalwart-stores-evidence-2026-07-30.md](docs/research/stalwart-stores-evidence-2026-07-30.md).

## No assumed operator acceptance

- Scaffold, research, and agent prose are **not** operator acceptance.
- Prefer **proposed**, **scaffold default**, **research finding**, **open**,
  **operator-deferred**, or **operator direction YYYY-MM-DD** until the
  operator explicitly approves something further in chat or writing.
- Do not say "we decided" or "locked" without that approval.
- Operator direction files are dated working direction, not eternal law.

## Packages and forks

- Detail: [docs/packages-and-forks.md](docs/packages-and-forks.md).
- Stalwart patches: Surmount fork `SurmountSystems/stalwart` when needed.
- Consume via flake input URL+rev (operator bumps) or operator-managed path
  submodule. **Agents never `git commit` / `git push` the fork** unless the
  operator explicitly instructs that work.
- Do not clone the fork if network/git policy blocks it; document the pattern.

## How agents work here

- Multi-file research, diagnosis, or non-trivial implementation: use
  **hierarchical subagents** (coordinator holds goals; workers own greps/edits;
  join on disk under `.grok/joins/`). Parent thread stays thin.
- Update living docs in the same turn as design changes.
- New durable stores: update [docs/DATASTORES.md](docs/DATASTORES.md) inventory.
- Agents never `git commit` (human-signed only). No bulk find-and-replace.
- ASCII only in docs we own; no em dashes.
