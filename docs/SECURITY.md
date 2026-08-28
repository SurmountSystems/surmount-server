# Security posture

Defense-in-depth notes for the Surmount mail VPS. Design notes:
[open-choices.md](open-choices.md). Secrets detail: [SECRETS.md](SECRETS.md).
Data plane: [DATASTORES.md](DATASTORES.md). Ops: [OPS.md](OPS.md).

**Last updated:** 2026-08-26 (Cargo.lock SHA-1/MD5 inventory: no sha1/md5
crates; product TLS is rustls+aws-lc-rs TLS 1.3; ring is locked as a
rustls optional dep). Prior 2026-08-20 (extra mailbox domains get the same
mail-auth DNS **and** the same hosted DNSSEC as the primary; leftover
parent DS is a SERVFAIL class to clear then sign, not leftover-unsigned).
Prior 2026-08-19 (sshd host keys: module advertises/generates
ed25519 only; live host still offers RSA until a deploy. Journald paper
trail size-capped; request log still omits Authorization/Cookie;
Onion-Location + Alt-Svc on Axum HTTPS; live Arti unit active; Tor
Browser verify still residual)

This document is posture and design, not a penetration-test report. Open
implementation details are marked honestly. Not operator-accepted unless you
say so.

### Host security elevation ladder (B0-B7)

Enabling already-shipped modules on a real box is **operator host work** with
proof gates. Sample `#mail-vps` keeps public HTTPS, `requireDeployMaterial`,
Arti, ban enforce, and production Nostr **commented** so eval needs no host
PEMs. Ordered stages (B0 preserve SSH access through B7 shrink transitional
fail2ban), proof columns, and D1 hybrid TLS honesty:
[deploy-host-local.md](deploy-host-local.md) section 6. Day-one automation:
[OPS.md](OPS.md). Residual host rows: [RESIDUAL.md](../RESIDUAL.md).

## Goals

- Run mail and product HTTPS on hardware/VPS **we control**, rebuildable from
  a flake, without a required third-party reverse-proxy CDN.
- Separate **deploy secrets**, **human vault**, and **disk encryption** so
  none is asked to do the others' job.
- Authenticate product operators/users with **Nostr cryptography** at our
  edge; keep Stalwart as the mail engine with ordinary mail credentials.
- Prefer **LUKS2** at rest when install allows; harden the live system
  regardless.

## Hash inventory (Cargo, 2026-08-26)

Best-effort `crates/Cargo.lock` parse (444 packages): **no** crate named
sha1/md5. Product rustls features `aws-lc-rs` + TLS 1.3 ServerConfig.
`ring` is still *listed* under rustls 0.23.42 in the lockfile; we did
not prove the UI binary links ring SHA-1. Git object IDs and Nixpkgs C
libraries are out of this lockfile. Full method and leftovers:
[research/hash-primitives-lockfile-2026-08-26.md](research/hash-primitives-lockfile-2026-08-26.md).

**Automated:** `just audit` is hermetic `cargo-audit` against flake
input `advisory-db` (RustSec; `--no-fetch --stale`). `just deny` is
`cargo-deny` bans (no sha1/md5 crates) and **is** in `checks.*.ci`.
`just audit` is in `checks.*.ci`. `h2` is 0.4.16 (RUSTSEC-2026-0258
cleared). Workspace `age` is 0.12 (drops the rekey path of
RUSTSEC-2026-0173). Three unmaintained warnings remain and are **not**
cargo-audit-ignored so `just audit` still prints them: instant via
nostr 0.44.8 (RUSTSEC-2024-0384), paste via leptos 0.8.20
(RUSTSEC-2024-0436), proc-macro-error2 via leptos_macro / rstml
(RUSTSEC-2026-0173). See
[research/cargo-audit-2026-08-26.md](research/cargo-audit-2026-08-26.md).
Bump the DB with `nix flake update advisory-db`.

## Non-goals (this doc)

- Live Vaultwarden unit / first admin user (S7b host residual after token;
  S7a module offline done; A0 `host-cutover --with-vaultwarden` scripts
  install kind + enable fragment; sample host stays enable=false).
- Full public / onion exposure design for VW (operator residual; no invented
  Q-ARTI answers).
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
- Loopback only: management-ui, Stalwart HTTP, Vaultwarden (when enabled;
  default rocket 127.0.0.1:8222; world bind needs allowNonLoopbackListen).
- Edge: nginx transitional-to-delete -> Rust target (operator direction
  2026-07-30); UDS preferred for local backends. See EDGE_AND_TLS.md.
- Hardening: `modules/hardening.nix` (SSH, light fail2ban; optional
  `accessControl` nft sets). When hardening is on, sshd **hostKeys** is
  exactly one ed25519 key at `/etc/ssh/ssh_host_ed25519_key`. The
  daemon used to also offer RSA (NixOS 26.05 default is rsa-4096 +
  ed25519). Clients already preferred ssh-ed25519. This module stops
  generating and advertising rsa. Live deploy is leftover: the running
  host still advertises RSA until a `deploy-host` switch. Dropping rsa
  from `hostKeys` does **not** delete leftover RSA files on disk.
  **PQ honesty:** dropping RSA host keys is classical SSH hygiene.
  Ed25519 is not post-quantum. OpenSSH host keys are not PQ. PQConnect
  is a separate userspace path; it is not a host-key swap. Do not claim
  this change is PQ. Ban path: rate limit + ban decide (`ban.rs`),
  enforcement default off. Target remains merciless ban + whitelist +
  last-used; Q-ACL-* open. See
  [research/access-control-fail2ban.md](research/access-control-fail2ban.md)
  and [EDGE_AND_TLS.md](EDGE_AND_TLS.md) operator end-to-end recipes
  (`nix run .#e2e` / `nix run .#e2e-host`, or `just e2e` / `just e2e-host`).
  Not a full WAF and not Cloudflare-dependent.
- PQC: management-ui **rustls + aws-lc-rs** with `prefer-post-quantum` offers
  hybrid **X25519MLKEM768** first among default kx groups (**D1**; hermetic
  unit proof of provider groups, not automatic live-host hybrid negotiation).
  Host probe after B1: `nix run .#surmount-tls-hybrid` (env
  `SURMOUNT_E2E_BASE_URL`; exit 2 BLOCKED if unset; not CI cutover).
  PQConnect is a **separate** path-layer track (**D2**;
  not a substitute for D1). Research:
  [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md); dual-pin
  [EDGE_AND_TLS.md](EDGE_AND_TLS.md), [deploy-host-local.md](deploy-host-local.md).
- **Mail auth classical dual-sign (operator direction 2026-08-11):** Day-1
  DKIM is **Ed25519 + RSA-4096** (selectors `stalwart` / `stalwart-rsa`).
  Best algorithms receivers accept today. **Neither is post-quantum**; larger
  RSA does not buy meaningful quantum resistance (Shor). Do not claim PQ from
  RSA-4096. When IETF/receivers support PQ or hybrid mail auth, Surmount
  tracks that residual separately from D1 TLS hybrid and D2 PQConnect.
  Checklist and paths: [DNS.md](DNS.md) *Hardness and quantum honesty*.
- **Mail records on every mailbox domain (operator 2026-08-20):** extra
  domains this host sends or receives through (examples:
  `cryptoquick.com`, `baxterartworks.com`) get the **same** mail-auth DNS
  as the primary, **including primary-class DNSSEC** (Namecheap hosted
  **DNSSEC Status ON**; ECDSA P-256 SHA-256 algorithm 13 acceptable for
  now): working Namecheap hosted DNS, SPF, dual DKIM TXT, DMARC
  at the operator-directed policy (live mailbox `_dmarc` is
  `p=quarantine`; do **not** use `p=reject`; Baxter intended is
  `p=quarantine`; public `_dmarc.baxterartworks.com` may stay NXDOMAIN
  while EmailType is FWD), TLS-RPT, CAA, MTA-STS on names this cert and
  this host actually serve. Do **not** harden only `surmount.systems`.
  A leftover parent **DS** with no matching child DNSKEY is a DNSSEC
  honesty fail: validating resolvers SERVFAIL the zone (`cryptoquick.com`
  was that class). That leftover is a SERVFAIL **bug to clear then
  sign**, not a reason to leave the zone unsigned. We asked cryptoquick
  OFF only because leftover DS 2368 (alg 13, digest type 1 SHA-1) had no
  DNSKEY. Operator wants ON same as `surmount.systems`. Do **not** re-add
  2368 by hand. DS digest type 1 (SHA-1) is also a fail, even if a
  DNSKEY exists. Static-site-only extra vhosts are not automatically
  mail domains. Public MX on registrar eforward is a **fail** for a
  claimed mailbox (`domain-audit`; `--live set-mx` is not a flip while
  EmailType is FWD). Dual-pin: [DNS.md](DNS.md),
  [COMPACTION-PIN.md](COMPACTION-PIN.md), [../AGENTS.md](../AGENTS.md).
- **Arti onion/hidden services (REQUIRED):** services must be reachable via
  Arti HS alongside clearnet; HS keys never in git; not a clearnet edge
  replacement. Clearnet HTTPS advertises the onion with **Onion-Location**
  and **Alt-Svc** (apex, www, services, extra static Hosts, MTA-STS;
  same v3; `/_o/{host}` on non-console Onion-Location; process-start load;
  restart after hostname/env/map/static-vhost change). Live unit
  `surmount-arti-hidden-service` is **active** (2026-08-17); Tor Browser
  verify and operator HS backup remain residual. Do **not** claim B3 fully
  closed. [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md),
  [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7,
  [EDGE_AND_TLS.md](EDGE_AND_TLS.md).

## Identity: Nostr product auth (high level)

Proposed direction (open-choices). Product mode for operators is **Nostr** at
the Axum edge (not HTTP Basic, not a password farm).

### Public edge policy (2026-08-12)

| Situation | Required |
|-----------|----------|
| **Public** product HTTPS / non-loopback operator console (services Host) | `authMode = "nostr"` + host session secret + allowlist |
| Loopback / lab / `just dev` | `authMode = "off"` allowed (open console; not public-safe) |
| Apex / www Host | Public packaged SurmountSystems/site (not the console); orthogonal to Nostr gate |

When mode is **off**, middleware does **not** gate routes: anonymous visitors
see full console HTML and inventory APIs. That is **lab only**. Do not leave
the live services Host on auth-off.

**Host enable (B4):** install `session-secret` and `nostr-allowlist` on Domain
B, set private host-local `authMode = "nostr"` (plus paths and
`publicBaseUrl`), switch, prove login redirect / API 401 / health 200. Full
operator runbook and proof curls: [OPS.md](OPS.md) section *Production Nostr
auth on the public edge (B4)*. Secrets shapes: [SECRETS.md](SECRETS.md).
Edge wording: [EDGE_AND_TLS.md](EDGE_AND_TLS.md).

**Footgun guard (product):** public primary edge + auth-off is refused
(binary + Nix assert). Lab/CI keep auth-off on loopback.
**Live B4 (2026-08-12):** public services is Nostr-gated (anon `/` login,
`/api/v1/domains` 401, `/health` 200). Apex/www serve packaged
`SurmountSystems/site` (2026-08-18).
Q-AUTH-1 still open. Report: `.agents/reports/impl-auth-live-b4-switch.md`.

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
  fail-closed. Mode `off` default for local dev / loopback only. Full Q-AUTH-1
  (durable store, bootstrap UX, key-loss) still residual (do not invent product
  answers here).
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

### Mailbox password hashing (Stalwart 0.16.15)

The management UI does **not** hash mailbox passwords in the browser or in
Axum. `POST /api/v1/accounts/password` sends the plaintext secret only over
the already-authenticated operator session to loopback Stalwart
(`x:Account/set` `credentials.0` `@type Password`). The engine hashes.

Stalwart 0.16.15 (this repo pin in `nix/packages/stalwart-mail.nix`) uses
`Authentication.passwordHashAlgorithm`. The enum and singleton default are
**Argon2id**. Source (tag `v0.16.15`):

- `crates/registry/src/schema/enums.rs`: `PasswordHashAlgorithm` `#[default]
  Argon2id`
- `crates/registry/src/schema/structs_impl.rs`: `impl Default for
  Authentication` sets `password_hash_algorithm:
  PasswordHashAlgorithm::Argon2id`
- `crates/directory/src/core/secret.rs`: `hash_secret` for `Argon2id` calls
  `Argon2::default().hash_password`

Public docs (same default; accessed: 2026-08-14):
[Passwords](https://stalw.art/docs/auth/authentication/password/) and
[Authentication.passwordHashAlgorithm](https://stalw.art/docs/ref/object/authentication/#passwordhashalgorithm)
(`argon2id` | `bcrypt` | `scrypt` | `pbkdf2`).

Policy runs at **set time**. Existing empty mailbox credentials stay empty
until the operator sets a password. Changing the algorithm later does not
rewrite stored hashes; IMAP still verifies known prefixes (`$argon2`, and
others listed on the Passwords page).

Optional host pin (not auto-applied; default is already Argon2id):

```bash
stalwart-cli update Authentication --field passwordHashAlgorithm=argon2id
```

Verify: `stalwart-cli get Authentication` and confirm
`passwordHashAlgorithm` is `argon2id`.

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
| **B. Human vault** | Vaultwarden (S7a offline done; S7b scripted by host-cutover after token) | Operator passwords/TOTP/notes | Deploy secrets at rebuild; mail store crypto |
| **C. Disk** | LUKS2 | Offline disk/snapshot; unlock material **never in git** | Running OS compromise |

**NEVER secrets in git** (plain or ciphertext). [hygiene.md](hygiene.md).

**agenix:** not dual-stacked; sops-nix primary only.

**Vaultwarden** ([upstream](https://github.com/dani-garcia/vaultwarden/)):

- Surmount wrap: `surmount.vaultwarden.*` over stock `services.vaultwarden`
  (default enable false on sample host). Offline S7a shipped; S7b enable path
  scripted by A0 host-cutover after token (live unit host-gated). Detail:
  [SECRETS.md](SECRETS.md) section 5.
- Defaults: SQLite, loopback Rocket (`127.0.0.1:8222`), `SIGNUPS_ALLOWED`
  forced false, `configureNginx` refused (Axum-first edge; no nginx product
  edge for VW).
- **Public path (optional):** `managementUi.vaultwardenProxyEnable` reverse-
  proxies `/vault/` (configurable) on the Axum edge to loopback Rocket.
  Vaultwarden login is SoT on that path day-one (no Nostr gate). WebSocket
  Upgrade is forwarded. Admin CSP is not applied to proxied VW responses
  (frame denial still applied; do not iframe VW into the console). Prefer
  after public https edge is real. Console Open vault becomes same-origin
  path when proxy is on. Default off on sample host.
- Without path proxy: console still surfaces operator-published URL / loopback
  derive as an external link ("not reverse-proxied here").
- **Admin token:** host EnvironmentFile path only
  (`adminTokenEnvFile`, e.g. `/run/surmount-secrets/vaultwarden/admin.env`
  via `nix run .#secrets-install-host`). Never inline in Nix config /
  `extraConfig` (stock config is store-bound and world-readable). Not
  "sops-inline secret string" as the product path. Never log ADMIN_TOKEN
  through the path proxy.
- Backups of `/var/lib/vaultwarden` are critical when live (restic paths are
  operator residual until enable).
- Chicken-and-egg: needs domain B host file to start; cannot supply secrets to
  pure flake eval; never activation feed for other units.

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
- **CSRF (shipped):** cookie-authenticated mutations use **double-submit**
  except mailbox password, create mailbox, and grant-console. Session
  exchange and authenticated `GET /mail` set **HttpOnly** `surmount_csrf`
  (Path=/, SameSite=Lax, Secure when HTTPS) as a belt. `GET /mail` embeds a
  **session-bound** token (HMAC of the session cookie with purpose
  `surmount-csrf-session-v1`) in `#mailbox-csrf` / `#mailbox-create-csrf` /
  `#mailbox-grant-csrf` and is `Cache-Control: no-store`.
  `POST /api/v1/accounts/password`, `POST /api/v1/accounts`, and
  `POST /api/v1/accounts/console` verify `X-CSRF-Token` / JSON `csrf`
  against that session HMAC (constant-time). The CSRF cookie may be absent;
  session + matching hidden/header token is enough. Session + empty/wrong
  token is **403**. No session is **401**.
  `POST /api/v1/auth/logout` still requires double-submit cookie+header.
  `PATCH /api/v1/accounts/{id}` still requires double-submit when a session
  cookie is present (NIP-98-only or lab auth-off escape skips CSRF).
- **Create mailbox is Administrator only (2026-08-14).** No public signup.
  Auth-off create is **403** without the lab directory escape. Console User
  is **403** even if they POST the form. Stalwart create is always engine
  **User** (never Stalwart Admin from the portal). Reserved local-part
  `admin` is refused. Map file
  `/var/lib/surmount/console/accounts.json` is owner `surmount-ui` mode
  **0600**; writes are temp-then-rename; existing symlink, directory, or
  world-writable inode is refused. Role is re-read from the map on every
  request. Cookie stays `{ sub, exp }` only. Host allowlist keys with no
  map row stay Administrator. User npubs are not copied to the allowlist.
- **Attach npub is Administrator only (2026-08-20).** `/mail` Grant console
  login posts `POST /api/v1/accounts/console` with session-bound CSRF.
  Bech32 `npub1...` (or hex) is validated; garbage and nsec are refused
  without echoing the token. Directory listing may stay unavailable; the
  bind writes the console map so that npub can log into the services
  portal (AuthMode nostr). User npubs stay in the map, not the host
  allowlist. Q-AUTH-1 is unchanged.
- **Baseline security headers (shipped on management router):**
  `Content-Security-Policy` lean for SSR admin (`default-src 'self'`, no
  third-party hosts; `script-src` with per-request nonce for `/login`
  NIP-07 inline script; `style-src 'self' 'unsafe-inline'` for DOGE CSS;
  `frame-ancestors 'none'`), `X-Content-Type-Options: nosniff`,
  `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`.
  **Onion-Location + Alt-Svc (2026-08-17, every public Host 2026-08-20):**
  emitted on mapped HTTPS 2xx/3xx (apex, www, services, extra static
  Hosts, MTA-STS; same v3; `/_o/{host}` discriminator for non-console).
  Not on `.onion` Host, not on the `:80` redirect router, not on the
  local Arti cleartext bind. Mail unmapped. 4xx/5xx emit nothing.
  Mapping is loaded at process start (no hot-reload). Dump on
  `GET /api/v1/system` `onion_discovery` (admin-gated when Nostr on).
  Header presence is not a Tor Browser Alt-Svc upgrade proof.
- Full webmail CSP / attachment sandbox remains residual (hostile email HTML).
- Rate-limit auth endpoints at edge and app.
- Do not expose Stalwart admin on the public internet long-term.
- Attachments: size limits, content-type caution, no drive-by exec paths.
- **Operator-only request and TLS logs (journald).** The Axum edge writes
  structured `http_request` and `tls_handshake_failed` lines to journald
  (`surmount-management-ui`; info on stdout, WARN+ on stderr). They are
  not world-readable and are not dumped on any HTTP route (`/health` and
  `/api/health` stay status JSON only). Do not add a public log viewer or
  write access logs under `/var/log` / `/var/lib/surmount`. Operators
  read via `just host-logs` or `journalctl -u surmount-management-ui`
  over SSH. Fields omit query content, cookies, Authorization, PEM, and
  cert bytes. Peer IP is a journal field only. Persistent journal is
  size-capped (`surmount.logging`; scaffold SystemMaxUse 1G, not a guest
  disk SKU). Never log passwords, tokens, or HS keys. Detail:
  [OPS.md](OPS.md) *Logging*.

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
  | Axum + Leptos    |  optional /vault/ path    | Vaultwarden      |
  | Nostr verify     |  proxy to loopback Rocket | (Bitwarden API)  |
  | admin + webmail  |  (or console link only)  | passwords/TOTP   |
  +--------+---------+                           +--------+---------+
           | service tokens / JMAP                         ^
           v                                               |
  +------------------+                    host EnvironmentFile
  | Stalwart         |                    (domain B ADMIN_TOKEN)
  | mail + RocksDB   |                    never store-bound secret
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
  Arti onion/HS REQUIRED (alongside clearnet; live unit + discovery
  headers 2026-08-17; Tor verify / HS backup residual)
  single VPS (multi-host deferred)
```

## Related modules and docs

| Path | Role |
|------|------|
| `modules/hardening.nix` | SSH (ed25519 host key only; no rsa generate/advertise) / transitional fail2ban sketch; merciless target in header |
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
