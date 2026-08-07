# Security posture

Defense-in-depth notes for the Surmount mail VPS. Design notes:
[open-choices.md](open-choices.md). Secrets detail: [SECRETS.md](SECRETS.md).
Data plane: [DATASTORES.md](DATASTORES.md). Ops: [OPS.md](OPS.md).

**Last updated:** 2026-08-02

This document is posture and design, not a penetration-test report. Open
implementation details are marked honestly. Not operator-accepted unless you
say so.

## Goals

- Run mail and product HTTPS on hardware/VPS **we control**, rebuildable from
  a flake, without a required third-party reverse-proxy CDN.
- Separate **deploy secrets**, **human vault**, and **disk encryption** so
  none is asked to do the others' job.
- Authenticate product operators/users with **Nostr cryptography** at our
  edge; keep Stalwart as the mail engine with ordinary mail credentials.
- Prefer **LUKS2** at rest when install allows; harden the live system
  regardless.

## Non-goals (this doc)

- Full Vaultwarden production hardening checklist as code (module comes later).
- Actually reformatting a live VPS with LUKS this turn.
- Claiming FDE stops a malicious cloud hypervisor with live memory access.
- Claiming Vaultwarden encrypts Stalwart/RocksDB mail data.

## Threat model (sketch)

| Threat | Mitigations (layered) | Residual |
|--------|----------------------|----------|
| Stolen disk / volume snapshot offline | LUKS2 FDE; provider volume encryption if any | Hypervisor live peek; bad unlock key handling |
| Git leak of repo | **Zero secret material in public tree** (plain or ciphertext); secrets host/out-of-band only | Operator accidentally commits secrets; still scrub and rotate |
| Laptop age key theft | Operator OPSEC; optional separate deploy keys; Vaultwarden for day-to-day human secrets | If age key + host ciphertext both leak, deploy secrets burn |
| Stolen VPS root while running | SSH keys only; fail2ban; minimal ports; no CF as excuse for weak origin | Rootkits; 0-days; weak app auth |
| Credential stuffing on web UI | Nostr signatures (not password reuse); rate limits; allowlists | Stolen nsec; XSS stealing session |
| Mail protocol abuse | Stalwart limits, greylist/spam, fail2ban careful expansion | Volume floods; reputation if open relay misconfig |
| Backup exfil | restic encryption; separate restic password in sops + offline copy | Backup repo ACL mistakes |
| Supply chain (nixpkgs, crates) | flake.lock pins; prefer minimal deps; review upgrades | Upstream compromise |
| Legacy site RCE | **Static files only**; no PHP for old content | Edge bugs; upload pipeline mistakes |
| CDN/MitM dependency | No required Cloudflare hop; our TLS | Operator DNS mistakes |

## No Cloudflare required path

See [EDGE_AND_TLS.md](EDGE_AND_TLS.md) and open-choices.

- A/AAAA for mail and services point **at the VPS**.
- We own TLS termination, rate limits, and basic abuse controls.
- DNS may still use any registrar (including Cloudflare **DNS-only**).
- Do not design features that only work behind CF Access/Workers/Tunnel/WAF.

## Network and edge

- Public: 25, 465, 587, 993, 4190, 80/443 (and SSH on a chosen port).
- Loopback only: management-ui, Stalwart HTTP, future Vaultwarden.
- Edge: nginx transitional-to-delete -> Rust target (operator direction
  2026-07-30); UDS preferred for local backends. See EDGE_AND_TLS.md.
- Hardening: `modules/hardening.nix` (SSH, light fail2ban; optional
  `accessControl` nft sets). Edge: rate limit + ban decide (`ban.rs`),
  enforcement default off. Target remains merciless ban + whitelist +
  last-used; Q-ACL-* open. See
  [research/access-control-fail2ban.md](research/access-control-fail2ban.md)
  and [EDGE_AND_TLS.md](EDGE_AND_TLS.md) operator end-to-end recipes
  (`nix run .#e2e` / `nix run .#e2e-host`, or `just e2e` / `just e2e-host`).
  Not a full WAF and not Cloudflare-dependent.
- PQC: TLS hybrid KEX on Axum/rustls where available; PQConnect as separate
  path layer research
  ([research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md)).
- **Arti onion/hidden services (REQUIRED):** services must be reachable via
  Arti HS alongside clearnet; HS keys never in git; not a clearnet edge
  replacement. [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md),
  [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7.

## Identity: Nostr product auth (high level)

Proposed direction (open-choices).

### What we do

- Operators (later users) prove control of a Nostr keypair (**npub** public,
  **nsec** private, never stored by us).
- HTTP APIs prefer **[NIP-98](https://nips.nostr.com/98)**: ephemeral event
  kind `27235`, tags `u` (absolute URL) and `method`, optional `payload`
  SHA-256, short `created_at` window; header
  `Authorization: Nostr <base64(event)>`.
- **Foundation (2026-08-01):** management-ui verifies NIP-98 with **rust-nostr**
  (`nostr` crate; not JS NDK). Scaffold session is an HMAC-signed HttpOnly
  cookie (`SURMOUNT_SESSION_SECRET` host-only). Allowlist via
  `SURMOUNT_NOSTR_ALLOWLIST` (env) or optional `SURMOUNT_NOSTR_ALLOWLIST_FILE`
  (same parse rules; **env wins** when non-empty). Empty allowlist is
  fail-closed. Mode `off` default for local dev. Full Q-AUTH-1 (durable store,
  bootstrap UX, key-loss) still residual (do not invent product answers here).
- After login, **sessions** (cookie or server-side) are expected so SSR pages
  are usable without signing every GET (exact design open).
- **[NIP-42](https://nips.nostr.com/42)** is relay AUTH; relevant if we speak
  relay protocol, not a drop-in browser session by itself.

### What Stalwart does

- Holds **mail accounts** and protocol auth (passwords, app passwords, OAuth
  device flows as configured).
- Supports external **OIDC/LDAP/SQL** directories
  ([OIDC backend](https://stalw.art/docs/auth/backend/oidc/)) and can act as
  OIDC provider/client. That is **not** the same as Nostr.
- **Does not** natively verify Nostr npub signatures unless we build a bridge.

### Bridge pattern

```text
Browser --NIP-98/session--> Axum/Leptos --maps npub--> role + mailbox
                                |
                                +-- service creds / managed tokens --> Stalwart
MUA --app password--> Stalwart (IMAP/SMTP)   [issued via admin after Nostr login]
```

Open: first-operator bootstrap allowlist, key-loss recovery, whether Surmount
ever becomes an OIDC IdP that Stalwart trusts.

## Secrets: three layers

Full write-up: [SECRETS.md](SECRETS.md). Summary:

| Layer | Tool | Protects | Does not replace |
|-------|------|----------|------------------|
| **A. Deploy** | sops-nix + age (host-local) | Service secrets on host at activation; **never in public git** | Human UX vault; FDE |
| **B. Human vault** | Vaultwarden (planned) | Operator passwords/TOTP/notes | sops at rebuild; mail store crypto |
| **C. Disk** | LUKS2 | Offline disk/snapshot; unlock material **never in git** | Running OS compromise |

**NEVER secrets in git** (plain or ciphertext). [hygiene.md](hygiene.md).

**agenix:** not dual-stacked; sops-nix primary only.

**Vaultwarden** ([upstream](https://github.com/dani-garcia/vaultwarden/)):

- nixpkgs: `services.vaultwarden` (config via env / module options).
- Reverse-proxy behind our edge on a private hostname; **no public signup**;
  admin token from sops; backups of vault data are critical.
- Chicken-and-egg: needs sops secrets to start; cannot supply secrets to
  pure flake eval.

## LUKS2 full disk encryption on NixOS VPS

**sops-nix does not unlock LUKS2.** Deploy-secret decrypt runs at activation
after root is mounted. Research (patterns, chicken-and-egg):
[research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

### Capability

NixOS supports LUKS/LUKS2 well:

- `boot.initrd.luks.devices.*`
- [Full Disk Encryption wiki](https://wiki.nixos.org/wiki/Full_Disk_Encryption)
- [Remote disk unlocking](https://wiki.nixos.org/wiki/Remote_disk_unlocking)
  (SSH/dropbear in initrd)
- **disko** for declarative partition+LUKS layouts
- **nixos-anywhere** can upload disk encryption keys at install
  ([howto](https://nix-community.github.io/nixos-anywhere/howtos/secrets.html))
- `systemd-cryptenroll` for TPM2/FIDO2 where hardware exists

### VPS reality

- Many provider "NixOS images" boot **unencrypted root**.
- FDE usually means **custom install**: nixos-anywhere + disko, custom ISO,
  or rescue-environment reformat (downtime).
- Most VPS **lack a useful TPM** for unattended unlock you control.
- Headless unlock choices:

  | Method | Pros | Cons |
  |--------|------|------|
  | Passphrase via initrd SSH | Simple, well documented | Manual after every reboot/crash; network must come up in initrd |
  | Tang/Clevis | Unattended if Tang up | Extra infra; network trust story |
  | TPM2 enroll | Unattended on bare metal | Rare/absent on VPS; PCR fragility; weak vs stolen whole machine |
  | Host-local keyfile on unencrypted boot | Convenience | Often **weaker** (key next to ciphertext); never from git |
  | sops-nix decrypt of keyfile on root | N/A for root unlock | **Too late**; root already must be open |
  | Any unlock material in public git | Forbidden | Plain or ciphertext; do not suggest |

### Recommended Surmount posture

1. **Prefer** providers/workflows that allow LUKS2 root via disko +
   nixos-anywhere (or equivalent) on greenfield or planned reinstall.
2. Plan **initrd SSH + passphrase unlock** as the default remote story
   (Option 1) unless better hardware appears; document NIC modules and
   static/DHCP in initrd.
3. Keep LUKS passphrases / recovery keys **offline** (paper, USB, human vault
   not only on this disk). **Never in git** (plain or encrypted).
4. Treat provider "encrypted volume" checkboxes as optional extra, not a
   substitute for OS LUKS we understand.
5. **Interim:** running on an unencrypted provider image is allowed with
   residual risk accepted until reinstall; do not pretend FDE exists.
6. Do **not** claim sops-nix configures FDE or unlocks root.
7. Do **not** store LUKS keyfiles as sops ciphertext in the public repo.

### What FDE does and does not do

**Does:** raise cost of offline disk theft, abandoned disks, many snapshot
exfil scenarios when the volume is locked.

**Does not:** stop root on a running system, malicious host ops with memory
access, ransomware already executing as root, or replace application auth.

## Auth failure to ban matrix (management-ui)

Accurate to code in `crates/management-ui/src/main.rs` (not aspirational
Q-ACL-1 surface answers). Enforcement still depends on
`SURMOUNT_BAN_ENFORCEMENT` (`off` = signal ignored, `dry-run`/`enforce` =
record; whitelist never banned). Hermetic tests:
`surface_audit_404_and_501_do_not_auto_ban`,
`auth_failure_ban_matrix_signals_and_skips`.

| Surface | Condition | Calls `signal_unauthorized`? |
|---------|-----------|------------------------------|
| `POST /api/v1/auth/session` | Event parse fail (missing/malformed body or Authorization) | **Yes** |
| `POST /api/v1/auth/session` | NIP-98 verify fail (sig, allowlist, skew, `u`, method) | **Yes** |
| Protected HTML/API (mode=nostr) | Missing session cookie and no `Authorization: Nostr` | **No** (gate only: redirect `/login` or 401) |
| Protected path | `Authorization: Nostr` present and error is BadSignature, NotAllowlisted, WrongKind, Skew, UrlMismatch, or MethodMismatch | **Yes** |
| Protected path | `Authorization: Nostr` present but unparseable (Malformed) | **No** (log gate only; not in ban match list) |
| Unknown HTML path | mode=nostr: temporary redirect to `/login` (gate); mode=off: 404 | **No** |
| Unknown `/api/...` path | mode=nostr without credentials: 401 gate | **No** |
| `POST /api/v1/jmap` | 501 when reached (auth off or authenticated); under mode=nostr without credentials the gate returns 401 first | **No** ban either way |
| `GET /health` | Always public | **No** |

Structured logs on auth fail use surface + `auth_error_kind` only (no cookie,
nsec, Authorization, or event JSON). Full Q-ACL-1 (which other mail/HTTP
surfaces must raise the hook) remains open.

## Application hardening (product UI / webmail)

- Nostr auth + short-lived NIP-98 windows; bind session to expectations
  (HTTPS, Secure cookies, sensible SameSite) when sessions exist.
- **Session cookie (shipped scaffold):** `surmount_session` is **HttpOnly**,
  **SameSite=Lax**, **Path=/**; **Secure** is set when listen mode is HTTPS
  (`secure_cookies` from `listen_mode.is_https()`). SameSite=Lax already
  blocks most cross-site POSTs from carrying the cookie.
- **CSRF (shipped):** cookie-authenticated mutations use **double-submit**.
  Session exchange sets non-HttpOnly `surmount_csrf` (SameSite=Lax, Secure
  when HTTPS) and returns `csrf` in the JSON body. `POST /api/v1/auth/logout`
  always requires cookie value to match `X-CSRF-Token` (or JSON body field
  `csrf`); mismatch or missing token is **403**. Account mutations
  (`POST /api/v1/accounts`, `PATCH /api/v1/accounts/{id}`) require the same
  when a session cookie is present (NIP-98-only or lab auth-off escape skips
  CSRF). Shared helper `require_csrf_double_submit`.
- **Baseline security headers (shipped on management router):**
  `Content-Security-Policy` lean for SSR admin (`default-src 'self'`, no
  third-party hosts; `script-src` with per-request nonce for `/login`
  NIP-07 inline script; `style-src 'self' 'unsafe-inline'` for DOGE CSS;
  `frame-ancestors 'none'`), `X-Content-Type-Options: nosniff`,
  `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`.
- Full webmail CSP / attachment sandbox remains residual (hostile email HTML).
- Rate-limit auth endpoints at edge and app.
- Do not expose Stalwart admin on the public internet long-term.
- Attachments: size limits, content-type caution, no drive-by exec paths.

## Backups and recovery (security-relevant)

- restic (or equivalent) for `/var/lib/stalwart-mail`, Vaultwarden data,
  and other state; password from sops. Prefer stop-service or FS snapshot
  consistency for RocksDB (DATASTORES.md).
- Separate offline copies of: age identities, LUKS recovery, restic password,
  Vaultwarden admin recovery.
- Test restore drills; a backup never restored is unproven.

## Intentional architecture diagram

```text
                     +---------------------------+
                     | Operators / users         |
                     | - Nostr nsec (client)     |
                     | - Vaultwarden clients     |
                     +-------------+-------------+
                                   |
           +-----------------------+-----------------------+
           | HTTPS (our edge)                              | (human apps)
           v                                               v
  +------------------+                           +------------------+
  | Axum + Leptos    |                           | Vaultwarden      |
  | Nostr verify     |                           | (Bitwarden API)  |
  | admin + webmail  |                           | passwords/TOTP   |
  +--------+---------+                           +--------+---------+
           | service tokens / JMAP                         ^
           v                                               |
  +------------------+                           sops supplies VW
  | Stalwart         |                           admin/DB secrets
  | mail + RocksDB   |
  +--------+---------+
           |
  =========+==========  LUKS2 (preferred) wraps block device
  | root FS + data   |
  | deploy decrypt-> |  age key on host; secrets host-local only
  | /run/secrets     |
  ===================

  public git: ZERO secrets (plain or ciphertext)
  LUKS unlock material: never in git
  no required Cloudflare hop
  Arti onion/HS REQUIRED (alongside clearnet; module residual)
  single VPS (multi-host deferred)
```

## Related modules and docs

| Path | Role |
|------|------|
| `modules/hardening.nix` | SSH / transitional fail2ban sketch; merciless target in header |
| `modules/networking.nix` | Firewall ports |
| `modules/secrets.nix` | sops-nix wiring |
| `modules/web.nix` | edge TLS (transitional nginx) |
| [SECRETS.md](SECRETS.md) | Full secrets story |
| [OPS.md](OPS.md) | Logs, scripts, runbooks |
| [EDGE_AND_TLS.md](EDGE_AND_TLS.md) | Edge/TLS/no CF; Arti HS required |
| [COMPACTION-PIN.md](COMPACTION-PIN.md) | Compaction reload pin |
| [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md) | sops vs LUKS |
| [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md) | PQConnect + TLS PQ |
| [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md) | Arti HS + VW vs SM |
| [research/access-control-fail2ban.md](research/access-control-fail2ban.md) | Ban/whitelist design |
