# Join: security / PQC / access control research dump

**Date:** 2026-07-30
**Worker:** research + living docs (no product module implement beyond comments)
**Workspace:** surmount-server

---

## Direct answers

### A. Does sops-nix unlock or configure LUKS2?

**No.**

- sops-nix decrypts deploy secrets at **NixOS activation** (age/gpg on host)
  into paths like `/run/secrets/*` after root is mounted.
- LUKS2 unlock is **initrd** (passphrase, keyfile, TPM, Tang, etc.) before
  multi-user activation.
- Upstream sops-nix: **does not fully support initrd secrets**.
- You can store a LUKS keyfile as sops ciphertext in git, but something must
  decrypt it in initrd (custom) or you unlock independently of sops.
- Chicken-and-egg for root FDE + unattended reboot is real: need human,
  TPM, network unlock, or weaker keyfile-on-boot.

**Doc:** `docs/research/luks2-and-deploy-secrets.md`

### B. PQConnect

- Latest release: **1.2.1** (2024-12-27). Repo: jedisct1/pqconnect.
- What: application-independent **post-quantum path encryption** between
  peers that both run it; DNS announcement; not a VPN-to-middlebox only.
- **Not in nixpkgs** (locked flake nixpkgs, nixos-25.05, nixos-unstable eval).
- Fit: complements Axum TLS and mail; optional operator/power-client layer.
  rustls 0.23.x + aws-lc-rs already has **X25519MLKEM768** etc. for TLS PQ KEX.

**Doc:** `docs/research/pqconnect-and-pqc.md`

### C. TLS posture (operator wants)

Documented in EDGE_AND_TLS + pqconnect research: :80->:443; no SSLv3/1.0/1.1;
prefer TLS 1.3; PQ hybrid where stack allows; no third-party proxy product;
CA candidates listed (LE, ZeroSSL, Buypass, GTS, commercial ACME); not locked.

### D. Merciless access control

Design sketch: rate limit + immediate/near ban on unauthorized + whitelist
never banned + last-used + prefer Rust/nft over Python fail2ban long-term.
`hardening.nix` header points at research; fail2ban remains light sshd sketch.

**Doc:** `docs/research/access-control-fail2ban.md`

---

## Files written

| Path | Role |
|------|------|
| `docs/research/luks2-and-deploy-secrets.md` | sops vs LUKS; VPS patterns |
| `docs/research/pqconnect-and-pqc.md` | PQConnect + TLS PQ + CA list |
| `docs/research/access-control-fail2ban.md` | Ban/whitelist design |

## Living docs updated

- `docs/operator-direction.md` (edge TLS/PQC/ACL sections + open ids)
- `docs/SECRETS.md` (sops does not unlock LUKS)
- `docs/glossary.md` (deploy secrets, LUKS2, sops-nix, PQConnect)
- `docs/EDGE_AND_TLS.md` (TLS posture, CA, ACL layers)
- `docs/SECURITY.md` (LUKS/sops, ACL, PQC pointers)
- `docs/principles.md` section 6
- `docs/open-choices.md` (new open ids)
- `modules/hardening.nix` header + fail2ban comment

## Open question ids added

Q-LUKS-1..4, Q-PQC-1..4, Q-ACL-1..6, Q-CA-1..2 (plus existing Q-HOST/TLS/EDGE).

## Not done (by design)

- No fail2ban/pqconnect/nft full module implementation
- No git commit
- No CA product lock
