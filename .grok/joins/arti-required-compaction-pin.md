# Join: Arti REQUIRED + COMPACTION-PIN

**Date:** 2026-07-30
**Scope:** Operator correction: Arti HS is required (not optional). Compaction
survival master doc. No product code. No git commit. ASCII only.

---

## Done

### Task 1: Arti optional -> REQUIRED

Updated every living place that called Arti optional / optional anonymity:

| Path | Change |
|------|--------|
| `docs/research/arti-and-secrets-manager.md` | Rewrote: HS **required**; Q-ARTI-1 answered; secrets never in git; VW SM gap |
| `docs/operator-direction.md` | Arti row REQUIRED; compaction pin link; Q table; related updates note |
| `docs/STACK.md` | Goals + ownership + edge bullets + related table |
| `docs/open-choices.md` | Directed table + Arti section + Q-ARTI-* + doc layout |
| `docs/EDGE_AND_TLS.md` | Header bullets + section "Arti onion / HS (REQUIRED)" |
| `docs/SECURITY.md` | Network/edge + diagram + related |
| `docs/principles.md` | 5b packaging + section 6 security + doc hierarchy |
| `docs/hygiene.md` | 8b Arti required; doc survival; anti-patterns |
| `docs/glossary.md` | **Arti**, **Hidden service / onion service**, **Onion service** |
| `docs/SECRETS.md` | VW-not-SM paragraph: Arti required, never in git |
| `README.md` | Bullet + Architecture table first row |
| `AGENTS.md` | Compaction pin; Arti required; no CF products |

Older join `.grok/joins/arti-pqc-mail-docs.md` still says "optional" in its
historical summary. **Living docs + COMPACTION-PIN win** over that join prose.

### Task 2: Compaction master

**New:** `docs/COMPACTION-PIN.md`

Sections: identity, absolute hygiene, versions (re-verify), host, mail/data,
edge/security, **Arti required**, identity/secrets, product UI, packages,
full doc index, open Q-* list, explicit non-claims, how to use after compaction.

### Task 3: Wiring

- README Architecture: COMPACTION-PIN linked **first**
- operator-direction: compaction reload companion + Arti required
- This join path

---

## Explicit non-claims (unchanged reality)

Still **not** implemented in code: Arti NixOS module, Axum edge cutover,
merciless ban, Leptos, VW module, LUKS install, pqconnect flake input (sibling
uncommitted), system rocksdb link, etc. Documented in COMPACTION-PIN section 13.

---

## Operator bar captured

- Arti HS **required** product surface alongside clearnet
- Still no Cloudflare products; own stack
- HS secrets: host deploy secrets + VW human inventory; never git
- Bitwarden Secrets Manager API gap on Vaultwarden noted honestly
