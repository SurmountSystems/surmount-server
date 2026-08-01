# Join: Q1-Q6 operator answers -> durable docs

**Date:** 2026-07-30
**Worker:** docs update from operator follow-up answers
**No git commit.**

## What changed

Recorded operator answers and renamed "seal" jargon to plain **encrypted
deploy secrets** / **secrets available at NixOS activation**. Axum-first edge.
New TLS trust research. poolWorkers stay default 16 on directed host.

### Answers landed

| Q | Answer summary | Where |
|---|----------------|--------|
| **Q1** Provider/LUKS | Operator asks provider about NixOS+LUKS2; assume NixOS OK; do not invent provider name | operator-direction §9, open-choices Q-HOST-1/2 |
| **Q2** Stalwart fork | SurmountSystems/stalwart; **flake input** required; agents never git fork | operator-direction, packages-and-forks, principles |
| **Q3** "Seal" | Explain + rename; two buckets only (deploy secrets + Vaultwarden); LUKS = disk encryption third | SECRETS, glossary, hygiene, secrets/README, STACK, README |
| **Q4** Edge | **Axum-first** HTTPS edge (in-process or surmount-edge crate); not separate proxy product by default; nginx still delete | EDGE_AND_TLS, rust-edge top note, STACK, AGENTS, modules headers |
| **Q5** ACME/CA | Do not lock ACME-only; research tradeoffs | **new** research/tls-trust-and-acme.md; EDGE pointer; Q-TLS-* |
| **Q6** poolWorkers | Leave default (= logical CPUs = 16 here); not a 32-core ask; optional 32-core SKU note | operator-direction §1-2, glossary, open-choices |

### Files touched

- `docs/operator-direction.md` (answers section; open ids; seal kill)
- `docs/SECRETS.md`, `docs/glossary.md`, `docs/principles.md`
- `docs/packages-and-forks.md`, `docs/open-choices.md`
- `docs/EDGE_AND_TLS.md`, `docs/research/rust-edge-and-uds.md`
- `docs/research/tls-trust-and-acme.md` (**new**)
- `docs/STACK.md`, `docs/hygiene.md`, `docs/DNS.md`, `docs/OPS.md`
- `docs/DATASTORES.md`, `docs/architecture-review.md`
- `README.md`, `AGENTS.md`, `secrets/README.md`
- `modules/web.nix`, `modules/hardening.nix`, `modules/management-ui.nix` (headers)

### Remaining opens (namespaced)

| Id | Topic |
|----|--------|
| Q-HOST-1 | Exact provider/SKU |
| Q-HOST-2 | LUKS day-one vs interim plain disk |
| Q-DEP-1 | sops-nix long-term vs other deploy-secrets tool |
| Q-EDGE-1 | Shared cert PEMs vs in-process issuance |
| Q-EDGE-2 | UDS path layout |
| Q-TLS-1..5 | Cert trust / DANE / multi-CA / short-lived (research) |
| Q-AUTH-1 | Session store / bootstrap allowlist / key-loss |

### Language

- ASCII, no em dashes, no ADR
- "Seal" only appears where explaining the rename / anti-jargon rule
- Prefer **deploy secrets**, **Axum-first edge**
