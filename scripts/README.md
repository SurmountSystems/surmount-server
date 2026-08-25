# Operator smoke

Small helpers for DNS, TLS, and mail posture. End-to-end SoT is flake apps
(Rust), not free-floating bash.

See [docs/OPS.md](../docs/OPS.md), [docs/EDGE_AND_TLS.md](../docs/EDGE_AND_TLS.md),
[RESIDUAL.md](../RESIDUAL.md) Validation SoT, and [docs/hygiene.md](../docs/hygiene.md).

## End-to-end (flake apps; preferred)

| Entry | Purpose |
|-------|---------|
| `nix run .#e2e` / `just e2e` | Local comprehensive **end-to-end** (no NixOS): hermetic `cargo test -p surmount-management-ui` + named critical test anchors + Rust pure-helper unit tests + optional Tor deep row. Matrix labels are package green + anchors, not five independent filters. |
| `nix run .#e2e-host` / `just e2e-host` | Host **end-to-end** probes. Requires `SURMOUNT_E2E_HOST=1` (else exit **2**) and `SURMOUNT_E2E_BASE_URL` (health FAIL if unset). Ban track needs `SURMOUNT_E2E_LAB_IP` unless `SKIP_BAN=1`. Summary `ban_drop=UNPROVEN` is not live drop. **Never** a flake check. |

Implementation: `crates/surmount-e2e` (`surmount-e2e`, `surmount-e2e-host` bins).
Pure helpers + host-gate contracts: `cargo test -p surmount-e2e --lib`
(also `checks.*.e2e-pure-test` inside `checks.*.ci`).

## Ops smoke (flake apps)

Product `scripts/*.sh` drivers are gone. Use `nix run .#...`.

| Entry | Purpose |
|-------|---------|
| `nix run .#surmount-domain-audit` / `just domain-audit` | Read-only DNS/web/TLS/mail posture (SPF/DKIM/DMARC/MX/HTTPS/optional SMTP STARTTLS). Default DKIM selectors include `stalwart` + `stalwart-rsa`. Extra mailbox apexes auto-require SPF, dual DKIM TXT, DMARC, TLS-RPT, CAA. Registrar eforward public MX is a FAIL on claimed mailbox domains (and EmailType FWD if `SURMOUNT_DOMAIN_AUDIT_EMAIL_TYPE` is set). Extra mailboxes do not require MTA-STS until the cert covers `mta-sts.<apex>`. Static-site vhosts are not auto mailbox domains. |
| `nix run .#surmount-tls-hybrid` / `just check-tls-hybrid` | **D1** hybrid KEX negotiation probe after B1 (requires `SURMOUNT_E2E_BASE_URL`; exit **2** BLOCKED if unset; not a flake check). |

No secrets. Safe to run from a laptop against public DNS/HTTPS where noted.
Local e2e uses temp self-signed certificate/key files inside cargo tests; host
mode expects operator-placed PEMs on the VPS. Never commit keys or `.onion`
addresses (Tor failure logs are onion-redacted).

**Honesty:** `nix run .#e2e` / `just e2e` green is not public cutover, production
`surmount-arti` ownership, or a live kernel firewall drop. Host rows need
`nix run .#e2e-host` / `just e2e-host`. Host green with `ban_drop=UNPROVEN`
proves helper + set preflight + lab membership only when ban track is on.

```bash
nix run .#e2e
# or: just e2e

SURMOUNT_E2E_HOST=1 \
  SURMOUNT_E2E_BASE_URL=https://127.0.0.1 \
  SURMOUNT_E2E_LAB_IP=203.0.113.50 \
  nix run .#e2e-host
# or: just e2e-host

# HTTPS-only host rows:
SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=https://127.0.0.1 \
  SURMOUNT_E2E_SKIP_BAN=1 nix run .#e2e-host

nix run .#surmount-domain-audit -- --no-color surmount.systems
# extra mailbox (same mail-record checks; MX flip parked, not a fail):
nix run .#surmount-domain-audit -- --no-color cryptoquick.com
nix run .#surmount-domain-audit -- --no-color baxterartworks.com
# force mailbox checks, or web-only posture:
nix run .#surmount-domain-audit -- --mail-domain extra.example
nix run .#surmount-domain-audit -- --static-site btcfur.com
# optional STARTTLS on highest-priority MX (outbound TCP/25 may be blocked):
nix run .#surmount-domain-audit -- --smtp --no-color surmount.systems
# or: just domain-audit
# or: just domain-audit -- cryptoquick.com
# or: just domain-audit -- --smtp --no-color surmount.systems
```

`surmount-domain-audit` is **not** a flake check (live public DNS would be flaky).
Exit: 0 clean, 1 warnings only, 2 failures, 64 bad invocation. Unsigned
DNSSEC (no DS) is INFO; leftover DS without DNSKEY is FAIL; DS digest
type 1 (SHA-1) is FAIL even with DNSKEY. Missing MTA-STS with no marker
is INFO. Interpret against [DNS.md](../docs/DNS.md) earn-trust vs Day-1
checklist.
