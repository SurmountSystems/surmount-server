# Join: Arti / PQC / mail legitimacy / no-Python-NPM docs

**Date:** 2026-07-30
**Scope:** Document operator directions A-D. No code. No git commit.
**ASCII only.** No secrets in examples.

**SUPERSEDED on Arti optional wording:** later same day, operator made Arti
onion/HS **REQUIRED**. Living docs + `docs/COMPACTION-PIN.md` + join
`arti-required-compaction-pin.md` win over any "optional Arti" prose below.

---

## Done

### A. PQ TLS + Cloudflare research (not product)

- Added **Industry PQ TLS research we cite** section to
  `docs/research/pqconnect-and-pqc.md` with plain URLs:
  - https://blog.cloudflare.com/pq-2025/
  - https://blog.cloudflare.com/pq-2024/
  - https://blog.cloudflare.com/post-quantum-roadmap/
  - https://blog.cloudflare.com/post-quantum-for-all/
  - https://blog.cloudflare.com/towards-post-quantum-cryptography-in-tls/
  - https://blog.cloudflare.com/post-quantum-sase/
  - https://pq.cloudflareresearch.com/
  - https://developers.cloudflare.com/ssl/post-quantum-cryptography/
  - plus IETF hybrid refs (draft-ietf-tls-ecdhe-mlkem, RFC 9954)
- Explicit: **appreciate research; no Cloudflare products in stack**
- Split locked in prose: **rustls hybrid PQ KEX first-class**; **PQConnect
  separate path**
- Takeaways: hybrid KEX, two migrations (KEX vs PQ auth), TLS 1.3, ossification
- Updated: `EDGE_AND_TLS.md`, `tls-trust-and-acme.md`, `operator-direction.md`,
  `STACK.md`, `open-choices.md`, `principles.md` (PQ wording)

### B. Arti reverse proxy / relay + Vaultwarden

- **New:** `docs/research/arti-and-secrets-manager.md`
- Arti = Tor Project Rust Tor; optional onion rproxy / client / relay sketch
- **Accurate SM split:**
  - Bitwarden **Secrets Manager** = separate cloud product (`bws`, SM SDK)
  - Vaultwarden = password manager client API
  - VW does **not** implement Secrets Manager (discussions #5483, #5702;
    licensed feature per maintainers)
  - Surmount path: deploy secrets for Arti startup; VW for human inventory;
    optional `bw` CLI; do not claim "VW Secrets Manager API"
- Light updates: operator-direction, STACK, open-choices (**Q-ARTI-***,
  **Q-SEC-SM-***), SECRETS.md "what VW is not", EDGE_AND_TLS optional path

### C. Mail legitimacy

- Rewrote strength into `docs/DNS.md`:
  - **Earn trust** framing
  - Ordered checklist (identity, auth, transport, prove-it)
  - SPF, DKIM, DMARC, PTR, MTA-STS, TLS-RPT, DNSSEC, DANE/TLSA
  - Automate affordably; custom CA not locked (pointer to tls-trust)
- operator-direction product row for mail legitimacy
- STACK ownership + related links

### D. No Python / No NPM

- Canonical: `docs/principles.md` **section 5b** (merged duplicate 9b away)
- `docs/hygiene.md` **section 2b** + section 4 points at 5b
- STACK product languages; operator-direction already had stack language row
- Anti-patterns in hygiene for Python/NPM product deps

---

## Files touched

| Path | Change |
|------|--------|
| `docs/research/arti-and-secrets-manager.md` | **Created** |
| `docs/research/pqconnect-and-pqc.md` | CF Research section + hybrid/PQConnect split |
| `docs/research/tls-trust-and-acme.md` | Automate CA; DANE; link PQ research |
| `docs/EDGE_AND_TLS.md` | PQ first-class; CF research-only; Arti optional |
| `docs/DNS.md` | Earn-trust framing + checklist + DANE |
| `docs/operator-direction.md` | PQC/CF, Arti, SM wording, mail legitimacy, open ids |
| `docs/STACK.md` | Ownership, CF cite, PQ, Arti, languages, DNS |
| `docs/open-choices.md` | Arti/SM/PQC/language opens |
| `docs/SECRETS.md` | VW is not Bitwarden Secrets Manager |
| `docs/principles.md` | 5b strengthened; PQ wording; removed dup 9b |
| `docs/hygiene.md` | 2b + anti-patterns; section 4 de-duped |
| `.grok/joins/arti-pqc-mail-docs.md` | This join |

---

## Not done / open (on purpose)

- No modules, packages, or code for Arti/PQConnect/edge
- Open ids left open: Q-ARTI-*, Q-SEC-SM-*, Q-PQC-*, Q-TLS-2 (DANE timing)
- No git commit (human-only)

---

## Status language

All of the above is **operator direction / research finding / design sketch**,
not claimed as locked eternal product law beyond what operator-direction
already labels by date.
