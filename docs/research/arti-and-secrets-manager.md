# Research: Arti hidden services and secrets manager

**Status:** research finding + design sketch, updated for **operator direction
2026-07-30**: Arti onion/hidden service reachability is **REQUIRED**, not
optional.
**Date:** 2026-07-30
**Operator direction:** Surmount Server services must be reachable via
**Arti** (Tor Project Rust Tor) **onion/hidden services**. That is first-class
product surface alongside clearnet (where clearnet applies). Still no
Cloudflare products. Still own stack. Secrets for Arti follow the absolute
rule: **never in git**.

Living maps: [STACK.md](../STACK.md), [SECRETS.md](../SECRETS.md),
[operator-direction.md](../operator-direction.md),
[EDGE_AND_TLS.md](../EDGE_AND_TLS.md),
[COMPACTION-PIN.md](../COMPACTION-PIN.md).

---

## Naming (use the right product words)

| Name | What it is |
|------|------------|
| **Arti** | Tor Project's **Rust** implementation of Tor protocols (client today; onion services and related work; relay work in progress upstream) |
| **Onion / hidden service (HS)** | Tor service identity that clients reach as `*.onion` without exposing a clearnet listener for that path |
| **Vaultwarden** | Unofficial Bitwarden-compatible **password manager** server (self-host), Rust |
| **Bitwarden Password Manager** | Official vault product; client API that Vaultwarden largely implements |
| **Bitwarden Secrets Manager** | Separate Bitwarden **cloud** product for machine/DevOps secrets (projects, access tokens, `bws` CLI, Secrets Manager SDK). **Not** the same as the password vault |

Call machine/runtime secret retrieval a **secrets manager** path when that is
the intent. Do not say "Vaultwarden Secrets Manager API" as if it were a
first-class VW feature unless evidence changes.

---

## What Arti is (measured upstream posture)

Upstream: https://gitlab.torproject.org/tpo/core/arti/
Crate docs: https://docs.rs/arti/
Relay crate: https://crates.io/crates/arti-relay
Onion reverse-proxy crate: https://lib.rs/crates/tor-hsrproxy
Tor Project blog (Arti 2.0.0, relay/authority/RPC work):
https://blog.torproject.org/arti_2_0_0_released/

**Plain English:** Arti is Tor rewritten in Rust for embeddability and safer
memory properties than the historical C `tor` client. It is a **first-party
Tor Project** codebase, not a Surmount fork and not a Cloudflare-style CDN.

### Capabilities relevant to Surmount (research snapshot 2026-07-30)

| Capability | Role | Maturity note (verify before ship) |
|------------|------|-------------------------------------|
| **SOCKS proxy** | Client egress through Tor | Core client path |
| **HTTP CONNECT** | Experimental HTTP proxy on the client (`http-connect` feature in arti docs) | Treat as experimental until upstream marks stable |
| **Onion service + reverse proxy** | `tor-hsrproxy`: map onion service ports to local backends | **Required product pattern:** publish Surmount services over Tor HS |
| **Relay** | `arti-relay` + ongoing reactor/channel work (Arti 2.x blog) | Relay is under active development; do not claim full C-tor relay parity without re-check at pin time. Relay is a **separate** ops choice from required HS. |
| **Embeddable libraries** | `arti-client` and related crates | Good fit for Rust-native Surmount tooling later |

Arti is **not**:

- A replacement for the Axum HTTPS **clearnet** edge
- A mail MTA or spam plane
- Cloudflare or any CDN product

Arti **is**:

- The **required** way Surmount exposes **onion/hidden service** reachability
  for product services (first-class surface next to clearnet where clearnet
  applies)

### Design sketch: required Arti hidden services

```text
  Clearnet users                 Tor users / operators
       |                                |
       v                                v
  Axum-first edge (:443)         Arti onion / hidden service (REQUIRED)
  TLS + rate limit + PQ KEX         |
       |                            +-- reverse proxy (tor-hsrproxy style)
       +--UDS--> management-ui      +--UDS or loopback --> same local apps
       +--UDS--> vaultwarden        (UI, VW, selected admin surfaces)
       +-- mail ports on Stalwart (clearnet MX as usual;
            onion mail exposure is a separate open Q-ARTI-*)

  Later open: Arti client SOCKS/HTTP CONNECT for operator egress
  Later open: Arti non-exit relay (bandwidth/policy/ToS; not default Day-1)
```

**Product direction (required bar):**

1. Keep **clearnet mail + Axum HTTPS** as the direct-VPS public path (no
   required CDN).
2. **Require Arti onion/hidden services** so Surmount services are reachable
   over Tor. This is **not** a nice-to-have and **not** "optional anonymity."
3. Package via **Surmount package overlay** (prefer current Arti; nixpkgs lag
   is not a reason to stay old once chosen).
4. **Relay** mode remains a separate ops decision (abuse, bandwidth, provider
   ToS, exit vs non-exit). Default lean: **no public exit relay** on the mail
   VPS without explicit operator yes. Required bar is **HS reachability**, not
   "must be a relay."
5. **Code status (updated 2026-07-30):** `modules/arti-hidden-service.nix`
   generates a **management-publish** `arti.toml` (`[onion_services]` +
   `proxy_ports`; `state_dir` = host HS identity path; no private keys in
   tree). TCP backend defaults derive from `managementUi.listenAddress:port`
   when `backendAddress` is null (UDS optional). Lean onion backend is
   **cleartext** HTTP (warning if UI is `listenMode=https` on the same TCP
   target). `startDaemon` default false; complete lean path does not need
   `acceptIncompleteOnionConfig` (no effect this version). Capable gate:
   Surmount `pkgs.artiOnionService` (null `package` prefers it; passthru
   claim satisfies the gate without setting the option bool) **or** explicit
   `packageIsOnionServiceCapable = true` for a non-Surmount binary. Stock
   nixpkgs `pkgs.arti` is client-default and stays fail-closed without that
   claim (**unit active != onion published**). HS dir must exist and be
   writable by `surmount-arti`. Residual: **live Tor verify** (do not claim
   published from unit active); operator host keys/ownership (e.g. 0750
   surmount-arti:surmount-arti on onionServiceStateDir); cleartext local
   backend when UI is https-only; hardening smoke; Q-ARTI-*. Living SoT:
   [COMPACTION-PIN.md](../COMPACTION-PIN.md) section 7 / non-claims,
   [RESIDUAL.md](../../RESIDUAL.md), [EDGE_AND_TLS.md](../EDGE_AND_TLS.md).

### Secrets Arti may need (examples, names only)

Do **not** put real secrets in docs or examples.

| Secret class | Examples (placeholders) | Persist how |
|--------------|-------------------------|-------------|
| Onion service identity | `hs_ed25519_secret_key` material | Deploy secrets at activation and/or offline backup; highest sensitivity; **never in git** |
| Client auth (if used) | authorized client keys | Deploy secrets + human inventory |
| Relay identity keys | relay id / onion key material if relay enabled | Deploy secrets + offline |
| Control/RPC tokens | local RPC auth if enabled | Deploy secrets |
| Operator notes | "which onion maps to which UDS" | Human vault notes (Vaultwarden) |

**Bootstrap rule matches SECRETS.md:** anything required to **start** Arti
before humans are around is **deploy secrets** (bucket 1). Day-to-day
rotation notes and recovery checklists for humans live in **Vaultwarden**
(bucket 2). Onion private keys should also have **offline** backup off the
VPS disk. Public git: **zero** secret material (plain or ciphertext).

---

## Bitwarden Secrets Manager vs Vaultwarden (accurate split)

### Bitwarden Secrets Manager (cloud product)

Docs:

- Overview: https://bitwarden.com/help/secrets-manager-overview/
- Product: https://bitwarden.com/products/secrets-manager/
- SDK: https://bitwarden.com/help/secrets-manager-sdk/
- SDK source: https://github.com/bitwarden/sdk-sm

Provides projects, secrets, service accounts, **access tokens**, `bws` CLI,
and SDKs (core SDK in Rust with bindings). Aimed at injecting infrastructure
secrets at runtime without stuffing them into CI plaintext.

This is a **different product surface** from the Bitwarden **password manager**
vault API.

### Vaultwarden support (evidence)

Vaultwarden implements the **Bitwarden client (password manager) API**, not
the commercial Secrets Manager product API.

Collaborator statements (GitHub discussions; re-check if bumping policy):

- https://github.com/dani-garcia/vaultwarden/discussions/5483
  Vaultwarden **does not** support Secrets Manager; described as a Bitwarden
  **licensed** feature.
- https://github.com/dani-garcia/vaultwarden/discussions/5702
  `bws` / Secrets Manager access tokens: **No** (same licensing reason).
- https://github.com/dani-garcia/vaultwarden/discussions/3368
  Longer thread; maintainers have not committed to implementing SM; SM not
  released under a license VW can reimplement like the PM client API.

**Conclusion for Surmount docs:** do **not** claim "Vaultwarden speaks the
Bitwarden Secrets Manager API" today. That would be false. Note the **API gap**
honestly when designing machine secret retrieval.

### What Vaultwarden *can* do for machine-ish workflows

| Approach | How | Honest label |
|----------|-----|--------------|
| **Password Manager vault items** | Store Arti-related notes, recovery seeds references, app passwords as normal ciphers | Human secrets manager UX, not SM API |
| **Personal / org API keys + official `bw` CLI** | `bw login` / unlock against VW; `bw get` / `bw serve` local Vault Management API | Automation via **PM API + CLI**, not Secrets Manager |
| **Community bridges** | e.g. projects that sync VW items into K8s secrets via CLI | Third-party; evaluate if ever needed (Surmount is not k8s-first) |
| **Bitwarden cloud Secrets Manager** | Real SM API/SDK | Only if operator accepts **Bitwarden cloud** (conflicts with self-host-first lean unless explicitly chosen) |
| **Deploy secrets (sops-nix today)** | Material on host at activation; **never in public git** | Correct path for Arti unit startup material |

Related Bitwarden PM automation (not SM):

- Vault Management API (via `bw serve`): https://bitwarden.com/help/bitwarden-apis/
- VW discourse note that public Bitwarden API differs; CLI local serve is the
  usual automation path: vaultwarden discourse "API basics"

### Recommended secrets story for Arti (design sketch)

```text
  Bucket 1 (deploy secrets / activation)
    - arti onion service key material paths
    - any RPC/auth tokens needed at systemd start
    - never required to call Vaultwarden HTTP at activation
    - never stored in public git (plain or ciphertext)

  Bucket 2 (Vaultwarden password manager)
    - human-facing inventory: "Arti onion address", recovery checklist,
      who holds offline key backup, rotation dates
    - optional: operator uses bw CLI to pull a cipher when doing a
      manual rotate (synced operator workflow)
    - NOT: claim Bitwarden Secrets Manager API compatibility on VW

  Offline
    - second copy of onion identity / recovery material off the VPS
```

If the operator later wants a **true** Secrets Manager API (projects, service
accounts, `bws`-shaped access tokens) self-hosted, options to research then:

1. **Wait / watch** Vaultwarden (unlikely soon per maintainer comments).
2. **Bitwarden self-host + licensed SM** if Bitwarden offers a self-host SM
   story the operator accepts (verify at decision time; do not assume).
3. **Surmount-owned small secrets service** in Rust (Nix + Rust product rule)
   speaking a narrow API we control.
4. Stay on **deploy secrets + VW PM** (often enough on a single VPS).

Open ids: **Q-ARTI-2** ... **Q-SEC-SM-1** below.

---

## Fit with Surmount principles

| Principle | Fit |
|-----------|-----|
| Nix + Rust product stack | Arti is Rust; package with overlay. Prefer no Python/NPM control plane |
| No required Cloudflare hop | Arti is unrelated to CF; clearnet stays direct-to-VPS |
| UDS local hops | Onion rproxy should target UDS/loopback backends like the clearnet edge |
| Two secrets buckets | Arti start keys = deploy secrets; human ops notes = Vaultwarden |
| Required HS surface | Arti onion reachability is **required product surface**, not opt-in garnish |
| Mail legitimacy | Clearnet MX earn-trust (SPF/DKIM/...) stays; onion does not replace DNS legitimacy |

---

## Open questions

**Q-ARTI-1.** ~~Enable Arti on the mail VPS at all?~~ **Answered (operator
2026-07-30):** **Yes, required** for onion/hidden service reachability of
Surmount services. Implementation timing still residual; direction is not
optional.

**Q-ARTI-2.** First surfaces over onion: private HTTP only (UI/VW), selected
admin, mail protocols, or phased set?

**Q-ARTI-3.** Any onion exposure of Stalwart admin or JMAP? Default lean:
**no** public onion mail admin without explicit yes.

**Q-ARTI-4.** Provider ToS / abuse posture for non-exit relay on the same IP
as MX? (Relay is optional; HS is required.)

**Q-SEC-SM-1.** Is Bitwarden Secrets Manager API a hard requirement, or is
deploy secrets + Vaultwarden PM + optional `bw` CLI enough?

**Q-SEC-SM-2.** If true SM API is required self-hosted, prefer wait on VW,
commercial Bitwarden path, or Surmount-owned Rust secrets service?

---

## Related

- https://gitlab.torproject.org/tpo/core/arti/
- https://blog.torproject.org/arti_2_0_0_released/
- https://github.com/dani-garcia/vaultwarden/
- https://bitwarden.com/help/secrets-manager-overview/
- https://github.com/dani-garcia/vaultwarden/discussions/5483
- https://github.com/dani-garcia/vaultwarden/discussions/5702
- [SECRETS.md](../SECRETS.md)
- [STACK.md](../STACK.md)
- [principles.md](../principles.md)
- [COMPACTION-PIN.md](../COMPACTION-PIN.md)
