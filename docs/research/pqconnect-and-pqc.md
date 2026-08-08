# Research: PQConnect and post-quantum connectivity

**Status:** research finding. **Not** a locked product choice.
**Date:** 2026-07-30 (PQ industry research section added same day)
**Operator interest:** E2EE PQC as a first-class priority; evaluate pqconnect;
enable **TLS hybrid PQ KEX** on the Axum/rustls edge where supported.

Living maps: [EDGE_AND_TLS.md](../EDGE_AND_TLS.md),
[SECURITY.md](../SECURITY.md),
[research/tls-trust-and-acme.md](tls-trust-and-acme.md).

**Two PQ levers (do not mash):**

| Lever | Layer | Surmount lean |
|-------|-------|---------------|
| **TLS hybrid KEX** (e.g. X25519MLKEM768) | Ordinary HTTPS / TLS 1.3 | **First-class** on Axum + rustls (aws-lc-rs) |
| **PQConnect** | Separate path-layer E2EE PQC between supporting peers | **Separate** optional/research track; does not replace TLS |

---

## Industry PQ TLS research we cite (not product dependency)

Surmount **does not** use Cloudflare products in the stack (no orange-cloud,
Workers, Tunnel, CF WAF, or CF Access as required path). We **do** read
public research and deployment write-ups, including Cloudflare Research blog
posts on post-quantum TLS, because they publish large-scale hybrid KEX and
migration measurements useful to anyone terminating TLS.

### Primary Cloudflare Research / blog URLs (plain)

| URL | Topic (short) |
|-----|----------------|
| https://blog.cloudflare.com/pq-2025/ | State of the post-quantum Internet (2025); hybrid ML-KEM adoption scale |
| https://blog.cloudflare.com/pq-2024/ | State of the post-quantum Internet (2024); hybrid motivation; browser experiments history |
| https://blog.cloudflare.com/post-quantum-roadmap/ | Accelerated roadmap; target full PQ including **authentication** ~2029 |
| https://blog.cloudflare.com/post-quantum-for-all/ | Early "PQ key agreement for all" enablement narrative |
| https://blog.cloudflare.com/towards-post-quantum-cryptography-in-tls/ | Earlier TLS PQC experimentation context |
| https://blog.cloudflare.com/post-quantum-sase/ | Two migrations: **key agreement** vs **digital signatures**; hybrid ML-KEM framing |
| https://pq.cloudflareresearch.com/ | Research landing for PQ key agreement status |
| https://developers.cloudflare.com/ssl/post-quantum-cryptography/ | Deployed hybrid identifiers (docs; product-shaped but lists IETF hybrids) |

Related non-CF standards context:

| URL | Topic |
|-----|--------|
| https://datatracker.ietf.org/doc/draft-ietf-tls-ecdhe-mlkem/ | ECDHE + ML-KEM hybrid in TLS (IETF work) |
| https://datatracker.ietf.org/doc/rfc9954/ | Hybrid key exchange in TLS 1.3 (RFC 9954) |
| https://www.imperialviolet.org/2018/12/12/cecpq2.html | Google CECPQ2 experiment (historical hybrid) |

### Takeaways for Surmount (research synthesis, not CF product advice)

1. **Hybrid KEX first.** Industry practice pairs classical ECDH (usually
   X25519) with ML-KEM so a break in one algorithm does not collapse the
   session secret. Matches rustls **X25519MLKEM768** / similar groups.
2. **Two migrations.** (a) Key agreement / encryption against
   harvest-now-decrypt-later (HNDL) is the urgent widespread deploy.
   (b) Post-quantum **authentication** (signatures, certs) is harder (size,
   PKI, ossification) and on a longer industry clock.
3. **TLS 1.3 baseline.** PQ hybrid deploy stories assume modern TLS; disable
   ancient protocols (already our lean).
4. **Ossification and size.** Larger ClientHello / keys caused real middlebox
   and server bugs in large scans; test early rather than assume flip-switch
   safety. Own edge code must be tested with hybrid groups enabled.
5. **Authentication still classical for most WebPKI today.** Enabling hybrid
   KEX on our edge does not by itself make certificate signatures PQ.
6. **Appreciate research; keep independence.** Cite and learn; terminate TLS
   ourselves on the VPS with rustls; no required CF hop
   ([EDGE_AND_TLS.md](../EDGE_AND_TLS.md), [hygiene.md](../hygiene.md)).

---

## What PQConnect is

Upstream: https://github.com/jedisct1/pqconnect (git mirror)
Site/docs: https://www.pqconnect.net (full docs under `doc/`)

**Plain English:** PQConnect is an **application-independent network security
layer** that adds **post-quantum end-to-end encryption** (and aims at PQ
authentication and fast key erasure) between machines that both run it.

It is **not**:

- A drop-in replacement for HTTPS / rustls cipher config alone
- A mail-protocol (SMTP/IMAP) crypto upgrade inside Stalwart
- A VPN that only protects you to a middle proxy

It **is** closer to: install client and/or server daemons; announce support in
DNS; compatible peers automatically encrypt traffic on the path with PQ crypto
without rewriting every app. Unmodified apps benefit. Apps that already have
PQ get a second layer.

From upstream goals (doc/crypto.md, doc/readme.md):

1. Post-quantum **encryption** for as much traffic as possible, quickly
2. Post-quantum **authentication** (upgrade urgency noted; slower industry)
3. Fast post-quantum **key erasure** (short-lived keys; ~2 minute erasure goal
   stated upstream)

Sysadmin path: install server package, create server key, run
`pqconnect-server` under systemd, publish DNS records so clients detect
support. Optional `pqconnect-client` on the same host for outbound. Clients
see special addressing (docs describe `10.*` style detection in tests) and
tunnel to supporting servers over UDP-framed protected traffic.

Team includes Bernstein, Lange, Levin, Yang (academic PQ crypto lineage).
Software language in the mirror: primarily Python packaging/scripts around
the crypto stack (see repo; not a pure Rust in-tree Surmount component today).

---

## Version table (measured 2026-07-30)

Point-in-time packaging check. Living Surmount host is **nixos-26.05** (see
[version-audit.md](version-audit.md)); re-eval attributes on the living lock
before claiming channel packaging changed.

| Source | Version / result | Notes |
|--------|------------------|-------|
| **Upstream latest release** | **1.2.1** (tag `1.2.1`, published 2024-12-27) | https://github.com/jedisct1/pqconnect/releases/tag/1.2.1 |
| **Upstream tags** | `1.2.1` tip of listed tags | Repo still pushed (mirror activity into 2026) |
| **Surmount flake nixpkgs (then)** | rev `ac62194c3917...` (**nixos-25.05** lock that day) | No `pqconnect` attribute |
| **nixpkgs nixos-25.05 (then)** | `nix eval ...#pqconnect` **missing** | "Did you mean connect?" |
| **nixpkgs nixos-unstable (then)** | `nix eval ...#pqconnect` **missing** | Same |
| **nixpkgs code search (then)** | No package named pqconnect | False hits only (e.g. `PQconnectdb` in libgda) |

**Conclusion (as of 2026-07-30 check):** PQConnect was **not packaged in
nixpkgs** (25.05 or unstable). Surmount would need a **Surmount package
overlay** (FOD or source build) if we adopt it. Prefer current upstream;
nixpkgs lag is not a reason to stay missing forever once we choose to package.

Re-validate versions when touching pins (principles.md).

---

## How it might fit Surmount

| Fit | Role | Fit quality |
|-----|------|-------------|
| **General tunnel / path PQ** | Optional second layer for operator laptops and supporting clients talking to the VPS | Strong conceptual fit with "E2EE PQC first-class" |
| **HTTPS edge** | Does not replace Axum TLS termination; can *wrap* paths when both ends run PQConnect | Complementary, not a substitute |
| **Mail (SMTP/IMAP)** | Could protect peer paths if both MTAs/MUAs sit behind PQConnect; most Internet mail peers will **not** run it | Partial; do not assume industry-wide |
| **Browser users** | Only if the browser host runs PQConnect client | Not transparent for random webmail users |
| **Internal service mesh** | Overkill vs UDS on one box | Poor fit for localhost hops |

Honest product framing:

- **Day-1 mail + HTTPS** still need classical TLS posture (TLS 1.3, good
  ciphers, cert path). See EDGE_AND_TLS and section C below.
- **PQConnect** is a **parallel track**: opt-in path protection for
  PQConnect-aware clients and our server announcement in DNS.
- **TLS hybrid KEX** (X25519MLKEM768 etc. in rustls) is a **different** PQ
  lever that helps ordinary browsers that speak hybrid TLS without installing
  PQConnect.

Recommended lean (research, not locked):

1. Own Axum edge with strong classical TLS + enable **hybrid PQ KEX as a
   first-class edge feature** when the stack supports it (rustls aws-lc-rs
   already exposes ML-KEM groups; see below). Aligns with industry hybrid
   practice summarized in the CF research section (without using CF).
2. Treat PQConnect as a **separate first-class research/optional module
   candidate** for operators and power clients, not as the only HTTPS story
   and not as a substitute for TLS hybrid KEX.
3. Do not block mail bring-up on PQConnect packaging.
4. Track PQ **authentication** (signatures/certs) as a longer industry wave;
   do not pretend hybrid KEX alone finishes "full PQ."

---

## TLS stack PQ today (Axum / rustls vs OpenSSL)

Surmount edge direction is **Axum + rustls** (not OpenSSL in-process by
default). management-ui already pulls rustls via reqwest `rustls-tls`.

### rustls (measured docs 0.23.43, 2026-07-29 release)

With **`aws_lc_rs`** crypto provider, key exchange groups include:

| Group | Role |
|-------|------|
| X25519, SECP256R1, SECP384R1 | Classical ECDH |
| **X25519MLKEM768** | Hybrid ECDH + ML-KEM (preferred hybrid pattern) |
| **SECP256R1MLKEM768** | Hybrid P-256 + ML-KEM |
| **MLKEM768**, **MLKEM1024** | ML-KEM-only KEM groups |

So: **rustls can do post-quantum hybrid key exchange today** when built with
aws-lc-rs and the edge configures those kx groups. The **`ring`** provider is
the classical-leaning path; prefer **aws-lc-rs** for PQ groups.

TLS protocol versions in rustls: modern stack; no SSLv3/TLS1.0/1.1 in normal
configs. Prefer **TLS 1.3 only** (or 1.2+1.3 only if a measured client forces
1.2; default lean TLS 1.3-only for public HTTPS when clients allow).

### OpenSSL 3.x

OpenSSL 3 has been growing provider-based PQ / hybrid support (oqs-provider
and upstream movement). Relevant if some **host tool** or mail binary links
OpenSSL. Stalwart binary FOD may embed its own TLS stack; do not assume our
rustls config rewrites Stalwart's mail TLS. Mail PQ may need Stalwart/openssl
or PQConnect path separately (**Q-PQC-3**).

### Operator TLS posture (map to product)

| Requirement | Lean |
|-------------|------|
| HTTP :80 gracefully upgrade/redirect to :443 | Yes; ACME HTTP-01 may share :80 |
| Enable PQ cipher/KEX where stack supports | rustls aws-lc-rs hybrid groups on Axum edge |
| Disable insecure SSL/TLS | No SSLv3, no TLS 1.0/1.1; prefer TLS 1.3 |
| No third-party reverse proxy as product edge | Already directed (Axum-first) |
| Better CA than LE if automated and not too pricey | Candidates below; not locked |
| E2EE PQC first-class (pqconnect) | Research + optional package track |

### Public CA candidates (automated ACME-class; not locked)

| CA | Notes |
|----|-------|
| **Let's Encrypt** | Default industry ACME; free; rate limits; great client support |
| **ZeroSSL** | ACME; free tier + paid; alternate trust path |
| **Buypass** | ACME-capable public CA; another alternate |
| **Google Trust Services** | ACME; public WebPKI |
| **Commercial ACME API** (DigiCert, Sectigo, etc.) | Paid; sometimes longer validation products; use if ops/legal need |

Multi-CA failover is **Q-TLS-4** in tls-trust-and-acme.md. Do not lock LE-only
or "never LE."

---

## Packaging sketch (if we adopt PQConnect)

Not implementing modules this turn. If directed later:

1. `nix/packages/pqconnect.nix` - FOD of upstream tarball or source build;
   pin version + hash; prefer current major.
2. NixOS module: `services.pqconnect-server` / client; DNS record checklist
   in OPS/DNS docs.
3. Firewall: allow required UDP/control ports per upstream after reading
   install scripts (do not guess port numbers into production without
   verify).
4. Document: PQConnect does not remove need for TLS on :443 or mail STARTTLS.

---

## Open questions

**Q-PQC-1.** Is PQConnect a Day-1 install on the mail VPS, or a Day-2 optional
after Axum edge + classical TLS are green?

**Q-PQC-2.** Who must run the client (operator laptops only vs any user)?

**Q-PQC-3.** For mail TLS PQ, prefer hybrid in Stalwart's stack when available,
PQConnect path, DANE, or combination?

**Q-PQC-4.** Package PQConnect in-repo overlay now as a stub, or wait for
explicit implement task?

**Q-CA-1.** Primary public CA for browser HTTPS at cutover (LE vs alternate)?

**Q-CA-2.** Worth dual-ACME failover on a single VPS, or one CA until pain?

---

## Related

- https://www.pqconnect.net
- https://github.com/jedisct1/pqconnect
- rustls 0.23.43 `crypto::aws_lc_rs::kx_group` (X25519MLKEM768, etc.)
- Cloudflare Research PQ posts (cited above; **not** a stack dependency)
- [EDGE_AND_TLS.md](../EDGE_AND_TLS.md)
- [research/tls-trust-and-acme.md](tls-trust-and-acme.md)
- [research/access-control-fail2ban.md](access-control-fail2ban.md)
- [research/arti-and-secrets-manager.md](arti-and-secrets-manager.md)
