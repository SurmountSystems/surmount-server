# Research: TLS trust, ACME, and centralized CAs

**Status:** research finding. **Not** a locked product choice.
**Date:** 2026-07-30
**Operator direction:** do **not** lock ACME-only or rustls-acme-only. Think
through centralized CA risk, supply chain, performance, and the split between
**mail TLS** and **browser HTTPS**.

Living product notes: [EDGE_AND_TLS.md](../EDGE_AND_TLS.md).
Edge implementation preference (Axum-first): operator-direction follow-up Q4.

---

## Why this note exists

Operator asked for deeper security and performance thought, especially:

- Re-evaluating **centralized public CAs** in an era of supply-chain attacks
- AI-assisted vulnerability discovery and weak disclosure culture
- Not treating "just use Let's Encrypt + rustls-acme" as the only future

No forced choice this turn. Open ids: **Q-TLS-1** ... below.

---

## Two planes that may differ

| Plane | Examples | Who must trust the cert |
|-------|----------|-------------------------|
| **Browser HTTPS** | `services.surmount.systems`, apex/www, Vaultwarden UI | Ordinary browsers and people; WebPKI trust store |
| **Mail TLS** | SMTP submission, IMAPS, STARTTLS on 25/587 | MUAs, other MTAs; partly WebPKI, partly DANE/DNSSEC, partly TOFU / operator config |

A design that is great for browser auto-renewal is not automatically right for
SMTP between strangers on the Internet. Document splits honestly.

---

## Option families (tradeoffs)

### 1. Public CA + ACME (Let's Encrypt and peers)

| | |
|--|--|
| **What** | Automated issuance via ACME (HTTP-01, TLS-ALPN-01, DNS-01). Browsers and most MUAs trust the CA by default. |
| **Pros** | Automatic renewal; huge client compatibility; operationally familiar; free/cheap at our scale |
| **Cons** | **Central trust:** compromise, mis-issuance, or policy change at the CA affects you; issuance **outage** can block renewals near expiry; account/key material and ACME client bugs are part of your supply chain; rate limits and validation path failures are real ops events |
| **Performance** | Handshake cost dominated by TLS stack and cert size/chain, not "ACME" per se. Renewal chatter is periodic (days), not per connection. OCSP stapling / short-lived certs change latency and failure modes. |
| **Fit** | Strong default for **public browser HTTPS** today. Common for mail TLS file paths too. |

Scaffold today uses NixOS `security.acme` with nginx (transitional). A future
Axum edge may use rustls-acme, instant-acme, or still consume PEMs from
`security.acme`. **None of those is locked.**

### 2. Private CA / internal PKI

| | |
|--|--|
| **What** | Surmount (or org) runs a CA; issues server certs you fully control. |
| **Pros** | Full control of policy, lifetime, revocation story; no public CA outage class for *your* issuance; can use short-lived certs aggressively |
| **Cons** | **Clients must trust your CA.** Fine for company-managed devices. Hard for **random Internet mail clients** and random browsers unless you ship/install trust anchors (usually unrealistic for public mail). |
| **Fit** | Possible for **admin-only** hostnames on managed operators' machines. Poor sole path for public MX/IMAP used by arbitrary MUAs. |

### 3. DANE / TLSA + DNSSEC (especially SMTP)

| | |
|--|--|
| **What** | Publish TLSA records in DNSSEC-signed zones so SMTP peers can authenticate the server cert (or raw keys) via DNS, not only via WebPKI. |
| **Pros** | Reduces sole dependence on public CAs for **mail transport** authenticity between supporting MTAs; aligns with "mail is not the browser" |
| **Cons** | DNSSEC + TLSA ops burden; uneven MTA support; does not replace browser WebPKI for HTTPS admin UI; misconfiguration can break deliverability |
| **Fit** | Worth serious later design for **mail TLS**, independent of browser ACME choice. Not a Day-1 blocker for bringing the host up. |

### 4. Short-lived certificates

| | |
|--|--|
| **What** | Hours-to-days lifetime instead of ~60-90 day LE defaults (where CA/profile allows). |
| **Pros** | Shrinks theft window; forces automation discipline |
| **Cons** | Higher renewal chatter and failure sensitivity; clients with broken clocks suffer more; still usually WebPKI-issued unless private CA |
| **Fit** | Interesting for high-security admin surfaces; measure ops cost on a single VPS. |

### 5. Pinning

| | |
|--|--|
| **What** | Clients pin SPKI/cert for known servers. |
| **Pros** | Strong for **first-party apps** you control |
| **Cons** | Hostile to general mail and random browsers; rotation is painful; HPKP on the public web is dead for good reasons |
| **Fit** | Limited: maybe future Surmount-native clients. Not general MX/IMAP Internet mail. |

### 6. Manual / long-lived purchased certs

| | |
|--|--|
| **What** | Buy or mint certs offline; install files; renew by calendar. |
| **Pros** | Simple mental model; no ACME client in the edge binary |
| **Cons** | Human renewal risk; still usually centralized CA; poor fit for automation culture we already have with flakes |
| **Fit** | Escape hatch, not preferred default. |

---

## Supply chain and disclosure culture (plain notes)

- **Public CA** is a high-value target. History has mis-issuance and trust
  removals. Diversifying issuance (backup ACME CA) reduces single-CA outage
  risk; it does not remove WebPKI centralization.
- **ACME clients and TLS libraries** (rustls, openssl, LE client code) are
  part of the TCB. Prefer maintained crates, minimal surface, and reproducible
  Nix builds. "AI found a bug" risk applies to *our* edge code too if we own
  Axum TLS termination.
- **Weak disclosure** means you may learn about client or CA issues late.
  Monitoring (cert expiry, failed renewal, TLS probe scripts in `scripts/`)
  matters as much as the brand of CA.
- **Do not** claim private CA is "safer" without stating who trusts it. For
  public mail, private CA often means **nobody trusts you** unless you also
  keep a public chain.

---

## Performance sketch

| Topic | Note |
|-------|------|
| Handshake | Session tickets/resumption, TLS 1.3, cert chain length, and CPU on small VPS matter more than ACME protocol choice at steady state. |
| OCSP / stapling | Stapling avoids client OCSP round-trips; must not become a hard outage if OCSP responder is down (soft-fail vs must-staple policy). |
| Renewal | ACME renewals are rare vs request rate. Burst risk is many names or failed loops. Rate limits at LE are operational, not per-connection latency. |
| In-process vs file PEMs | In-process ACME (rustls-acme style) couples edge restarts to cert state. File-based `security.acme` lets mail and edge share paths; two consumers must not fight renewals. |

---

## Practical split (proposal, not locked)

| Surface | Lean (research) | Why |
|---------|-----------------|-----|
| **Browser HTTPS** (`services.`, admin UI) | Public CA + automated issuance still the compatibility default; implementation open (NixOS ACME PEMs vs in-process) | Browsers will not install your private CA |
| **Mail TLS** (465/993/STARTTLS) | Often same public cert files as today; **evaluate DANE/TLSA + DNSSEC** for MTA-MTA strength as part of earning mail trust | MUAs vary; DANE is mail-specific leverage |
| **Internal-only admin** | Could experiment with private CA + operator trust store later | Closed client set |

Axum-first edge (operator preference) can terminate HTTPS and still **consume
shared PEM files** from a host ACME service, or run ACME in-process later.
Those are implementation choices under **Q-EDGE-1** / **Q-TLS-***, not a
requirement to pick rustls-acme on day one.

### Automate affordably; custom CA without locking

Operator lean: **automate** certificate issuance/renewal at a cost that fits a
single-operator VPS; stay **open** to custom or alternate CA paths without
freezing product law on one brand.

| Path | Automation | Lock? |
|------|------------|-------|
| Public ACME (LE, ZeroSSL, Buypass, GTS, ...) | High (ACME clients / `security.acme`) | **Not** locked to one CA |
| Commercial ACME API | High if paid tier worth it | Optional |
| Private / custom CA | High for managed clients; poor sole path for random MUAs/browsers | Open experiment for admin-only hosts |
| DANE/TLSA + DNSSEC (mail) | DNS + cert/key alignment automation | Complements WebPKI for SMTP peers; see [DNS.md](../DNS.md) |
| Manual long-lived files | Low | Escape hatch only |

**Hybrid PQ KEX** on the wire (rustls) is independent of which CA signed the
certificate. Industry hybrid/migration research (including Cloudflare Research
blog posts we cite for learning only) is summarized in
[pqconnect-and-pqc.md](pqconnect-and-pqc.md). Surmount still terminates TLS
on-origin; no Cloudflare product dependency.

---

## Open questions

**Q-TLS-1.** For public `services.surmount.systems` HTTPS, stay on public CA
automation indefinitely, or plan a private-CA path for operator-managed
clients only?

**Q-TLS-2.** For mail TLS, pursue DANE/TLSA + DNSSEC on what timeline (not Day-1,
or earlier if DNS is ready)?

**Q-TLS-3.** Prefer one shared cert pipeline (files under `/var/lib/acme` or
similar) feeding both edge and Stalwart, vs separate issuance per plane?

**Q-TLS-4.** Multi-ACME-CA failover (e.g. Let's Encrypt + backup CA) worth
the ops complexity on a single VPS?

**Q-TLS-5.** Short-lived certs: any surface where the security gain beats
renewal fragility for a solo operator?

**Q-EDGE-1** (also EDGE_AND_TLS): shared `security.acme` PEMs vs in-process
issuance on the Axum edge for web only.

No answer forced this turn. When the operator picks, update EDGE_AND_TLS and
operator-direction same turn.

---

## Related

- [EDGE_AND_TLS.md](../EDGE_AND_TLS.md)
- [DNS.md](../DNS.md) (earn-trust mail checklist; DANE/TLSA)
- [research/rust-edge-and-uds.md](rust-edge-and-uds.md)
- [research/pqconnect-and-pqc.md](pqconnect-and-pqc.md) (TLS hybrid PQ + PQConnect split; CF research citations)
- [SECURITY.md](../SECURITY.md)
- [operator-direction.md](../operator-direction.md) Q5 follow-up
