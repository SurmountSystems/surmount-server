# Operator scripts

Small helpers for DNS, TLS, and mail ports. End-to-end SoT is flake apps
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

## Ops smoke scripts (bash; secondary)

| Script | Purpose |
|--------|---------|
| `check-dns.sh` | dig-based MX/A/AAAA/TXT presence checks |
| `check-tls.sh` | openssl certificate dates and handshake for host:port (standalone ops; host e2e embeds the same check in Rust; **exit 1 if expired**) |
| `check-mail-ports.sh` | TCP connect checks for mail/HTTPS ports |

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

chmod +x scripts/*.sh   # once, if needed
./scripts/check-dns.sh
./scripts/check-tls.sh services.surmount.systems:443
./scripts/check-mail-ports.sh mail.surmount.systems
```
