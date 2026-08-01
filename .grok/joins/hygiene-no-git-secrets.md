# Join: hygiene - never secrets in git

**Date:** 2026-07-30
**Scope:** operator hygiene correction + stack language pin
**No git commit** performed (human-signed only).

## Directive (absolute)

- Public Surmount Server tree: **zero secret material** in git.
- Forbidden: plaintext secrets, `.env`, age/sops private keys, LUKS unlock
  material, and **ciphertext** of production/unlock secrets.
- Do **not** suggest "sops-encrypted LUKS keyfile in git" or any encrypted-
  secrets-in-public-git pattern.
- Deploy secrets: host / operator out-of-band only; sops-nix remains a
  possible **on-host** decrypt tool, not a git secret store.
- LUKS unlock: passphrase / initrd SSH / TPM; weaker host-local keyfile only
  if accepted in writing; never from git.

## Stack language pin

- Prefer **NixOS + Nix + Rust** for product/ops gaps.
- **No Python** in product/ops (fail2ban transitional-at-most).
- **No NPM** ecosystem.
- Short why in `docs/principles.md` section 5b.

## Files updated

| Path | Change |
|------|--------|
| `docs/hygiene.md` | Prominent top rule NEVER secrets in git; section 2/2b; anti-patterns |
| `docs/research/luks2-and-deploy-secrets.md` | Scrubbed git-keyfile suggestions; absolute ban; Option 1 default |
| `docs/SECRETS.md` | Section 0 absolute; host-local deploy model; anti-patterns |
| `docs/glossary.md` | Deploy secrets, LUKS2, sops-nix definitions |
| `docs/principles.md` | Section 0 secrets; 5b stack language; agents/git |
| `docs/operator-direction.md` | Identity/secrets table; Q3 hygiene note; related |
| `docs/SECURITY.md` | Threat model; layers; LUKS posture; diagram |
| `docs/open-choices.md` | Secrets buckets + Q-LUKS wording |
| `docs/STACK.md` | Secrets flow diagram |
| `docs/DATASTORES.md` | Deploy secrets inventory row |
| `docs/research/scaffold-assumptions-inventory.md` | Correction banner + section 10 |
| `secrets/README.md` | Placeholder only; host-local bootstrap |
| `modules/secrets.nix` | Comments: host paths, never commit secrets |
| `README.md` | Hygiene + secrets wording |
| `AGENTS.md` | NEVER secrets + stack language sections |
| `/home/hunter/.grok/AGENTS.md` | Global pin: never commit or suggest secrets |

## Not done

- No `git commit` in either repo.
- No change to sops-nix flake wiring beyond comments (still scaffold defaults).
- Operator still chooses real host secret placement when bringing up the VPS.
