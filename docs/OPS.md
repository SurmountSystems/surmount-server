# Operations and self-check posture

How the Surmount mail VPS should be operated, observed, and checked
end-to-end. Aligned with [hygiene.md](hygiene.md). Architecture:
[STACK.md](STACK.md).

**Last updated:** 2026-08-02
**Design notes:** [open-choices.md](open-choices.md) (self-ops)

## Day-one when the VPS is ready

Ordered host bring-up. Local `just e2e` is **not** cutover. Host
`ban_drop=UNPROVEN` is **not** live traffic drop.

### Production networking assumption (operator 2026-08-02)

On the production NixOS box, **TCP port 80 is free** so the product
**redirect-only** listener can bind (`redirectHttpToHttps` +
`httpRedirectListen`, default `0.0.0.0:80`). That port serves HTTP->HTTPS
redirect/upgrade only. No cleartext management API on :80.

| Assumption | Status |
|------------|--------|
| Free :80 for product redirect-only bind | **Operator-approved** for production day-one |
| ACME HTTP-01 on product :80 | **Still parked** (Q-EDGE). Free :80 does **not** invent ACME-on-product-:80. Prefer external PEMs, DNS-01, or dual-run ACME until Q-EDGE answers |
| Free :443 for product rustls HTTPS | Required for public cutover; place host PEMs first |

Firewall still opens only required ports (`modules/networking.nix`). Dual-run
escape (`surmount.web.enable = true`) owns :80 ACME/redirect while nginx is
active; product redirect-only and dual-run nginx must not both claim :80
(eval mutex). Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md).

### Bring-up order

1. **Place TLS PEMs** on the host (deploy secrets; never in git). Paths match
   `surmount.managementUi.tlsCertPath` / `tlsKeyPath` (see host
   [configuration.nix](../hosts/mail-vps/configuration.nix),
   [SECRETS.md](SECRETS.md), [EDGE_AND_TLS.md](EDGE_AND_TLS.md)).
2. **HTTPS management UI** with `listenMode = "https"` and product
   `web.enable = false` (Axum rustls edge). Enable redirect-only :80 when
   ready (`redirectHttpToHttps`; free port assumption above). Rebuild/switch.
3. **Rebuild and prove locally** first if needed (`just e2e`), then on host:
   `SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=https://... just e2e-host`.
   Require BASE_URL; lab ban needs `SURMOUNT_E2E_LAB_IP` unless
   `SURMOUNT_E2E_SKIP_BAN=1`. Summary `ban_drop=UNPROVEN` means set membership
   only, not proven live drop.
4. **Arti HS keys** under `onionServiceStateDir` (owned by `surmount-arti`).
   Start daemon when ready; unit active alone is not onion published. Optional:
   set `managementUi.onionUrl` or `onionHostnameFile` so the console can show
   the operator-published address ([RESIDUAL.md](../RESIDUAL.md)).
5. **Optional ban lab** on host sets (`surmount-ban4` / `surmount-ban6`) via
   e2e-host; cleanup with helper `remove-ban`.
6. **DNS and mail** cutover when edge is healthy: [DNS.md](DNS.md), mail ports,
   Stalwart principals via `stalwart-cli` / bootstrap admin.

Host module entry: [hosts/mail-vps/configuration.nix](../hosts/mail-vps/configuration.nix).
Open residual tracks: [RESIDUAL.md](../RESIDUAL.md).

## Intent

The stack should help the operator **troubleshoot itself**:

- Useful logs in journald without needing a SaaS sink as a hard dependency
- Health endpoints for the pieces we own
- Small scripts / flake apps for DNS, TLS, and mail checks
- Abuse controls we run (fail2ban, Stalwart, edge limits) without Cloudflare WAF
- Runbooks in `docs/` that match reality

Prefer boring, local tools. Later, scripts under `scripts/` may become
`flake.apps` or small Rust bins; keep them pure and secret-free.

## Logging

| Source | Where | Notes |
|--------|-------|-------|
| Stalwart | journald via stdout tracer (nixpkgs default) | Level configurable in settings; avoid ANSI in journal |
| management-ui | journald (systemd service) | Move toward structured logs (JSON or kv) in Phase 1 UI |
| nginx (transitional-to-delete) / future Axum-first edge | journald | Access/error as enabled |
| sshd / fail2ban | journald | Hardening module |

### Retention (direction)

- Use journald vacuum settings on the host (size and/or time) so disks do not
  fill silently. Exact `SystemMaxUse` belongs in host config once disk size
  is known.
- Do not log secrets (passwords, raw auth headers, private keys).
- Mail content does not belong in application info logs.

### Reading logs (operator)

```bash
journalctl -u stalwart-mail -e
journalctl -u surmount-management-ui -e
journalctl -u surmount-arti-hidden-service -e   # when startDaemon=true
journalctl -u surmount-arti-scaffold-status -e  # enable without daemon
journalctl -u nginx -e          # transitional-to-delete edge
journalctl -u fail2ban -e
```

### Arti onion / hidden service (ops)

| Item | Notes |
|------|-------|
| Package | Prefer `pkgs.artiOnionService` (Surmount overlay; `onion-service-service`). Stock `pkgs.arti` is client-default. |
| Config | `/etc/surmount/arti.toml` (generated; no private keys) |
| HS identity dir | `surmount.artiHiddenService.onionServiceStateDir` default `/run/surmount-secrets/arti/onion-service` |
| Ownership | **Must** be owned/writable by `surmount-arti:surmount-arti` (e.g. mode **0750**). Module does **not** auto-create this dir (`ConditionPathIsDirectory` gates the daemon; missing dir => inactive, not a restart loop). Root-owned 0700 can pass the path check then fail at keystore open. |
| Process cache | `/var/lib/surmount/arti` (+ `cache/`) via tmpfiles 0750 surmount-arti |
| Secrets | HS private keys **never in git**. Operator places identity on host only. |
| Honesty | `systemctl is-active surmount-arti-hidden-service` does **not** prove an onion is published on the Tor network. Live Tor verify remains residual. |

Example host prep (operator; paths are examples only):

```bash
install -d -m 0750 -o surmount-arti -g surmount-arti /run/surmount-secrets/arti/onion-service
# Place HS identity material under that directory per Arti/Tor docs.
# Never commit key material to the public repo.
```

## Health endpoints

| Check | How |
|-------|-----|
| UI liveness | `GET` management-ui `/health` (loopback or UDS; or via public HTTPS) |
| Stalwart reachability from UI | `GET /api/v1/stalwart/status` |
| Stalwart process | `systemctl status stalwart-mail` |
| Edge (product default) | `systemctl status surmount-management-ui` (`listenMode=https`); nginx should be inactive when `web.enable` is false |
| Edge (dual-run escape) | `systemctl status nginx` only if `surmount.web.enable = true` (see EDGE_AND_TLS.md) |

### nginx dual-run unused detection (checklist; no auto-delete)

Product default is **Axum-first** with `surmount.web.enable = false`. The
nginx module (`modules/web.nix`) remains as a **dual-run escape** until
operators confirm it is unused. **Do not delete `web.nix` without explicit
operator OK.**

Before asking to remove the module path, check on each relevant host and in
git config:

- [ ] Host flake / `configuration.nix` does **not** set `surmount.web.enable = true`
- [ ] `systemctl is-active nginx` is inactive (or unit absent) on the deploy host
- [ ] No operator runbook still requires nginx ACME or nginx :443 as the public edge
- [ ] Public HTTPS health works on management-ui rustls alone (`just e2e-host` when env set)
- [ ] No dual-run mutex conflicts still relied on (UI public https + web on is fail-closed at eval)
- [ ] Docs/search for `web.enable = true` / dual-run escape still accurate after delete
- [ ] Operator explicitly approves deleting `modules/web.nix` (and follow-on STACK/EDGE/OPS scrub)

Until those boxes are green and operator-approved, keep the dual-run escape.
Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md), residual track in
[RESIDUAL.md](../RESIDUAL.md).

Future: aggregate `/api/v1/ready` that fails if Stalwart HTTP is down (keep
`/health` as pure liveness if you use orchestrators that distinguish).

## Backups

Full store inventory and consistency rules: [DATASTORES.md](DATASTORES.md)
sections 7-8. Secrets offline copies: [SECRETS.md](SECRETS.md).

- Module: `modules/backups.nix` (restic, opt-in)
- Mail data plane: `/var/lib/stalwart-mail` (RocksDB under `db/`)
- Surmount state: `/var/lib/surmount` (import staging, future product state)
- ACME certs: `/var/lib/acme` (or edge-specific paths after cutover)
- Secrets: age/sops recovery is separate from restic; see `secrets/README.md`
  and [SECRETS.md](SECRETS.md) (deploy vs Vaultwarden vs LUKS layers).
  When Vaultwarden is enabled, include its data dir in backup paths.

**RocksDB consistency:** prefer `systemctl stop stalwart-mail` (or an atomic
volume snapshot with documented assumptions) before restic of `db/`. A live
file-level copy of an open RocksDB can restore torn state. We have not
load-tested backup windows under production mail volume.

Test restore before you need it (IMAP/JMAP readback of known messages).
Snapshot or restic **before** large Maildir imports ([MIGRATION.md](MIGRATION.md)).

## Abuse and perimeter (no CF WAF)

| Control | Role |
|---------|------|
| Firewall | Only required TCP ports (`networking.nix`) |
| fail2ban | SSH jail by default; expand only with clean filters (transitional) |
| Stalwart spam-filter + greylisting | Primary mail abuse path |
| Edge rate limits | HTTPS request floods ([EDGE_AND_TLS.md](EDGE_AND_TLS.md)) |
| Ban / whitelist (first path) | `surmount.accessControl` + management-ui ban layer; default off; see EDGE end-to-end |
| Kernel firewall `surmount_guard` sets | Opt-in empty sets (`surmount-ban4`/`surmount-ban6`); host load required for live drop |
| Axum auth -> ban signals | BanCandidate stub shipped; Q-ACL-1 open |
| SSH keys | Password auth off by default preference |

## Scripts inventory

Helpers live under repo `scripts/` (and some are installed by Nix modules).

| Script / command | Purpose |
|------------------|---------|
| `nix run .#e2e` / `just e2e` | Local comprehensive end-to-end (Rust flake app; hermetic cargo matrix; optional Tor). SoT |
| `nix run .#e2e-host` / `just e2e-host` | Host end-to-end (Rust flake app; `SURMOUNT_E2E_HOST=1` or exit 2; also requires `SURMOUNT_E2E_BASE_URL`; `LAB_IP` unless `SKIP_BAN=1`). **Never** a flake check |
| `scripts/check-dns.sh` | MX/A/AAAA/NS/TXT checks for a domain |
| `scripts/check-tls.sh` | TLS certificate dates and handshake for a host:port (standalone ops; host e2e embeds the same check in Rust) |
| `scripts/check-mail-ports.sh` | TCP reachability of mail-related ports |
| `surmount-mail-import-maildir` | Operator Maildir import (from `mail.nix`) |
| `stalwart-cli` | Upstream admin/import (from Stalwart package) |

End-to-end lives in `crates/surmount-e2e` and flake `apps.e2e` / `apps.e2e-host`.
Ops smoke scripts use `dig`/`openssl`/`nc` (devShell or VPS). They are
**read-only checks** unless documented otherwise. No secrets in env files
committed to git.

### Future packaging

Promote remaining ops smoke scripts to:

```text
apps.check-dns
apps.check-tls
```

or a small `surmount-ops` Rust crate if logic grows. Until then, bash stubs
document the hygiene bar.

## Operator runbook sketch

### First boot / DNS

1. [DNS.md](DNS.md) checklist (A/AAAA, MX, PTR, SPF, DKIM, DMARC)
2. `./scripts/check-dns.sh surmount.systems mail.surmount.systems`
3. ACME after HTTP 80 reachable: `./scripts/check-tls.sh services.surmount.systems:443`

### Mail path

1. `systemctl is-active stalwart-mail`
2. `./scripts/check-mail-ports.sh mail.surmount.systems`
3. Submit test message; read via IMAP client
4. Import: [MIGRATION.md](MIGRATION.md)

### UI path

1. `curl -fsS http://127.0.0.1:8090/health` (Surmount management UI default port)
2. `curl -fsS https://services.surmount.systems/health` when public HTTPS is live
3. Stalwart HTTP management: SSH tunnel to **8080** (first-boot default; prefer
   over public `/stalwart-admin/`). Example:
   `ssh -L 8080:127.0.0.1:8080 mail-vps` then open `http://127.0.0.1:8080`

### Incident: disk full

1. `journalctl --disk-usage` / vacuum if appropriate
2. Check `/var/lib/stalwart-mail` growth; internal FTS and LSM compaction can
   retain space after deletes until cleanup/compaction run (DATASTORES.md)
3. Confirm Stalwart DataRetention / blob cleanup schedules if expunge is not
   reclaiming space
4. Do not delete `db/` cold without a verified backup and restore plan

### Incident: cert expiry

1. `security.acme` / edge renew logs (dual-run path) or host PEM renew path
2. `./scripts/check-tls.sh ...`
3. If using ACME HTTP-01 on the **dual-run nginx** path, confirm port 80 is
   reachable for challenges. Product :80 is **redirect-only** by default;
   ACME HTTP-01 on product :80 remains **parked** (Q-EDGE). Free production
   :80 is for redirect bind, not an ACME invent.

## Local Nostr auth enable (`just dev`)

Default local console is open: `SURMOUNT_AUTH_MODE=off` (see `justfile`
`dev`). Foundation is rust-nostr NIP-98 + HMAC session cookie. **Not** JS NDK.
nsec never goes on the server.

To gate the local console with Nostr:

```bash
export SURMOUNT_AUTH_MODE=nostr
export SURMOUNT_NOSTR_ALLOWLIST=npub1...   # or hex pubkey; empty + mode=nostr = fail-closed
# optional file when env empty:
# export SURMOUNT_NOSTR_ALLOWLIST_FILE=/path/to/allowlist.txt
export SURMOUNT_SESSION_SECRET=$(openssl rand -hex 32)
# optional:
# export SURMOUNT_NIP98_MAX_SKEW_SECS=300
# export SURMOUNT_SESSION_TTL_SECS=...
just dev
```

Then open `http://127.0.0.1:8080/login` (or `SURMOUNT_LISTEN`). Use NIP-07 in
a capable browser extension, or exchange a kind 27235 event against
`POST /api/v1/auth/session`. Public always: `/health` and auth endpoints.

| Env | Role |
|-----|------|
| `SURMOUNT_AUTH_MODE` | `off` (default) or `nostr` |
| `SURMOUNT_NOSTR_ALLOWLIST` | Comma-separated npub/hex; env wins when non-empty |
| `SURMOUNT_NOSTR_ALLOWLIST_FILE` | Optional host file (same parse); used when env empty; unreadable = fail-closed |
| `SURMOUNT_SESSION_SECRET` | HMAC cookie signing; required when mode=nostr (lab: openssl; host: deploy secret / EnvironmentFile) |
| `SURMOUNT_NIP98_MAX_SKEW_SECS` | Optional skew window (default 300) |

Host Nix mirrors: `managementUi.authMode`, `nostrAllowlist`,
`nostrAllowlistFile`, `sessionSecretPath` (prefer path over inline env for
production). Auth-failure ban matrix: [SECURITY.md](SECURITY.md). Full product
answers still open under **Q-AUTH-1** (key-loss, durable session store,
first-operator bootstrap UX). Depth: [SEARCH_AND_UI.md](SEARCH_AND_UI.md),
[SECURITY.md](SECURITY.md).

Local green with mode=nostr is **not** public cutover and does not close
Q-AUTH-1.

## Live Stalwart directory list (optional)

Default accounts API is honest empty (`SURMOUNT_DIRECTORY` unset /
`unavailable`). Live list is **explicit opt-in** only.

| Piece | Role |
|-------|------|
| `managementUi.directory = "stalwart"` | Enable live management JMAP list |
| `managementUi.stalwartTokenPath` | Host path to **raw** API token file (preferred production) |
| Token file content | First non-empty non-`#` line is the Bearer token. Comments-only or empty = **fail-closed** at process start. Must be a regular file, mode **not** group/world readable (e.g. **0600**), readable by `surmount-ui` |
| `authMode = "nostr"` | **Required** with live directory (principal list must not be open). Lab escape: `allowDirectoryUnauthenticated = true` |
| `stalwartTokenEnv` | Lab only; requires `allowLabInlineStalwartToken = true` (eval fail-closed otherwise). Prefer path so secrets stay off the Nix store |

Module: missing token path uses `ConditionPathExists` (unit inactive, not
restart thrash). Present-but-bad content still fails at binary start under
`Restart=on-failure` (burst capped). List HTTP errors surface **status-code
only** in UI notes (no upstream body reflection).

Local green with live directory is **not** host cutover.

## Related modules

| Module | Ops relevance |
|--------|----------------|
| `hardening.nix` | SSH, fail2ban |
| `backups.nix` | restic |
| `networking.nix` | firewall |
| `web.nix` | edge (nginx transitional-to-delete; Rust target) |
| `mail.nix` | Stalwart + import helper |
| `management-ui.nix` | UI service |
