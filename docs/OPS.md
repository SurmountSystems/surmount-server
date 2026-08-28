# Operations and self-check posture

How the Surmount mail VPS should be operated, observed, and checked
end-to-end. Aligned with [hygiene.md](hygiene.md). Architecture:
[STACK.md](STACK.md).

**Last updated:** 2026-08-25 (docs polish: operator bins are `nix run .#...`;
living mailbox maps stay in `~/.agents/surmount-server/operator-facts.md`,
not this public file. Prior same day: every `nix/packages/surmount-*.nix`
is a flake package/app; `just` aliases only `nix run`; product `script/*.sh`
drivers are gone. Hermetic crate tests live in `checks.*.ci`). Prior
2026-08-24 (`just deploy` publishes static sites;
`just deploy-host` is the NixOS generation.) Prior 2026-08-22 (living mailbox map is
`~/.agents/surmount-server/operator-facts.md`, not this public file.
Prior 2026-08-20 (primary public MX **flipped**:
`surmount.systems` MX is `10 mail.surmount.systems`; EmailType **MX**;
SPF `v=spf1 a:mail.surmount.systems -all`; DMARC **`p=quarantine`**.
Laptop `--live set-mx` after operator Custom MX. Extra mailbox MX stays parked.
`--live set-mx` still fails closed while EmailType is FWD. MailPlus
uid-to-person maps, alias lists, and import counts live in operator-facts.
`admin@surmount.systems` stays the API-token principal, not a person
mailbox alias. Extra local Domains may exist for local delivery while
public MX is still parked. Mailbox password still operator on `/mail`.
Copy helper: `gio list` even when AFP `cur/` fails libc `[[ -d ]]`.)
**Prior:** 2026-08-19 (MailPlus import from DS3018xs for named extra people
as ordinary User only; no console Administrator. Primary hunter mailbox
was not re-imported from that NAS. Operator still sets mailbox passwords
on `/mail`. Copy `--host` matches IPv4 GVFS via laptop-private hint. Prior same day: DiskStation AFP: GNOME NetworkPassword `server` is host id `DS1513` / `DS3018xs` and `<id>.local`, never an IPv4. `--afp-host` / `--uri` may still be IPv4 for the mount URI only. After every `secrets-prompt` / `secret-tool` store, paste buffers are wiped (empty stdin). Live Secret Service migrate/remount is a later job. Prior same day: `secrets-prompt` / `diskstation-afp-mount` also store GNOME NetworkPassword so Nautilus and raw `gio mount` remember. `--host DS1513` or `DS3018xs` remains the Secret Service label. DSM rename is optional. mDNS is optional. Last-week path is gio by operator IPv4. Optional laptop-private hint `~/.local/share/surmount/diskstation-afp-hosts` (never git). Do not publish office LAN IPv4 in this tree. Prior same day: two office DiskStations host ids **DS1513** and **DS3018xs**. Discover `just diskstation-discover`. Store one `synology-afp` item per NAS. Mount `just diskstation-afp-mount -- --host DS1513`. Copy `just mailplus-copy-uid -- --host DS1513 <uid>`. Password never on argv. Prior same day: sshd host keys: `modules/hardening.nix` advertises/generates ed25519 only. Live host still offers RSA until a `deploy-host` switch. Do not wipe live keys this wave.)
**Prior:** 2026-08-18 (The operator laptop is local.
surmount-1 is the remote builder. Measure each machine on that
machine. Never treat laptop `inxi` as the mail host.
`/etc/nix/machines` lives on the laptop Nix client and describes
surmount-1. Laptop local `max-jobs` from this laptop `inxi` (16
threads if setting system `max-jobs`). Machines-file slots from live
guest `nix.settings.max-jobs` after `ssh surmount-1 inxi` / `lscpu`.
Do not copy laptop cores onto the mail-host module. Laptop system
`/etc/nix/machines` still needs a local sudo TTY.
Prior same day: `MemoryMax` + `Nice=19` on `nix-daemon.service`,
`cpuQuota=auto`, memory-safe `maxJobs`. Prior same day: laptop Let's Encrypt renew is
`just laptop-renew-cert -- --check|--live --directory production`;
issue only if due; laptop user timer, not VPS ACME. HTTPS is the Nix store management-ui binary;
the temporary `/run` ExecStart drop-in is gone. `surmount.remoteBuilder`
is enabled from private host-local. Lake is **not** part of this
mail-host NixOS module. Do not start Lake. Do not create a Lake unit.
SHC ticket 261 closed 2026-08-18. Prior same day: we started niced Lake
without `MemoryMax`; niceness is not a memory cap; that is why the box
went dark. Prior 2026-08-17: mail-plane TLS apply path shares Axum
Let's Encrypt PEMs; `mail` is on the production certificate hostname
list. **Live 2026-08-21:** `mail.cryptoquick.com` is on that same shared
leaf (IMAP :993 / SMTPS :465 YE2). Live IMAP is the production Let's
Encrypt leaf.)
**Design notes:** [open-choices.md](open-choices.md) (self-ops)
**Host-local + deploy driver:** [deploy-host-local.md](deploy-host-local.md)
**Host process view:** `just btop` opens interactive `btop` over Eternal
Terminal (`just et` stack). Needs `etserver` after `just deploy-host`.
**Host hardware probe:** `just host-inxi` runs `inxi` on that same SSH target
(no sudo, no TTY). Measure the mail host on the mail host. Laptop `inxi` is a
different machine. `--userland` uses `nix shell nixpkgs#inxi` until
`pkgs.inxi` (`modules/inxi.nix`) is live after deploy.
**Host hardware probe:** `just host-inxi` runs `inxi` on that same SSH target
(no sudo, no TTY). Measure the mail host on the mail host. Laptop `inxi` is a
different machine. `--userland` uses `nix shell nixpkgs#inxi` until
`pkgs.inxi` (`modules/inxi.nix`) is live after deploy.
**DiskStation AFP (laptop):** two NAS units. Host ids are **DS1513** (5-bay)
and **DS3018xs** (6-bay). Units are already configured. **DSM rename is
optional.** **mDNS is optional.** Last-week path is gio by an operator
IPv4 on the office LAN (do not paste those addresses here). Pass
`--host DS1513` or `--host DS3018xs` as the Secret Service label, plus
`--afp-host` (hostname or IPv4) or `--uri afp://hunter@<host>/MailPlus`.
Optional laptop-private hint (never git):
`~/.local/share/surmount/diskstation-afp-hosts` with lines
`DS1513=<afp-host>` and `DS3018xs=<afp-host>`. Discover on the office LAN
from the laptop is still available:
`just diskstation-discover` (Avahi `_afpovertcp._tcp` / `_http._tcp` /
`_smb._tcp` plus `getent hosts` for `DS1513.local` and `DS3018xs.local`;
no IPv4 printed; no nmap). Store one LAN password per NAS:
`just secrets-prompt -- synology-afp --host DS1513 --secret-service --no-staging`
and the same for `DS3018xs` (no-echo; never on argv; `secrets-install-host`
refuses this kind). That intake also writes GNOME NetworkPassword
(`protocol=afp`, `user=hunter`, `server` = host id and `<id>.local`, never
an IPv4). `--afp-host` / `--uri` / hint may be IPv4 for the mount URI only.
Optional `--afp-host` on secrets-prompt. After a successful
`diskstation-afp-mount`, missing NetworkPassword is filled from the same
lookup (stdin, never argv). After every store, paste buffers are wiped
(empty stdin; never the secret). DSM rename is still not required. Mount MailPlus:
`just diskstation-afp-mount -- --host DS1513` (URI example
`afp://hunter@DS1513.local/MailPlus`; or `--afp-host` IPv4 / `--uri`;
reuses that host's GVFS `*volume=MailPlus*`). Unmount:
`just diskstation-afp-mount -- --host DS1513 unmount`. Copy one numeric uid
from the NAS you mounted:
`just mailplus-copy-uid -- --host DS1513 <uid>` (gio list reconstruct; dest
`/var/lib/surmount/import/maildir/<uid>/<uid>/Maildir`; same SSH target as
`just btop`; do not print it). Do not assume which NAS holds MailPlus.
Import on the host: `surmount-mail-import-maildir`. Copy `--host` also
matches an IPv4-named GVFS volume when the laptop-private hint file maps
that host id (never print the IPv4). Named extra people are imported from
DS3018xs as ordinary User. Living map:
`~/.agents/surmount-server/operator-facts.md`. Do not guess other uids.
Depth: [MIGRATION.md](MIGRATION.md),
[SECRETS.md](SECRETS.md).
**Secrets custody + install bridge:** [SECRETS.md](SECRETS.md) (three domains;
Domain C = human custody only; cutover automation = A→B + host-local + DNS APIs;
`just secrets-prompt` for no-echo Domain A intake;
`nix run .#secrets-install-host` / `just secrets-install-host`;
**V4/S9** human vault: section 5.1 + optional export
`nix run .#secrets-export-bw-to-staging`; OPS one-pager below)
**Host cutover driver (A0 + V5 + ladder):** `nix run .#surmount-host-cutover`
/ `just host-cutover`
(default dry-run; `--step material|prep|install|free-443|dns|deploy|prove|le-prod|all`;
`--acme-path`, optional `--host-profile`, `--free-443`, `--with-vaultwarden`;
hermetic crate tests in `checks.*.ci`)
**Free Stalwart :443 (V1):** `nix run .#free-stalwart-public-443` /
`just free-stalwart-public-443`
(default dry-run; hermetic crate tests in `checks.*.ci`)

## Day-one when the VPS is ready

Ordered host bring-up. Local `just e2e` is **not** cutover. Host
`ban_drop=UNPROVEN` is **not** live traffic drop.

### Host cutover gates (A0 living checklist)

Ordered gates for a real box. **S7a** (Vaultwarden module + UI offline) is
**done**. **S7b** is live VW token install + enable + unit smoke, owned by the
cutover driver after material exists (not "S7a host residual").

| Gate | What | How (offline scripts) |
|------|------|------------------------|
| **G1** | Material present in private staging | `nix run .#surmount-host-material-inventory` / `host-cutover --inventory-only` |
| **G2** | Install domain B secrets | `secrets-install-host` (or cutover / deploy `--install-secrets`) |
| **G3** | Units start | `systemctl is-active` after `deploy-host` switch |
| **G4** | Public TLS | host PEMs **or** ACME path; `just e2e-host` when `BASE_URL` set |
| **G5** | :80 redirect-only | product redirect listener; e2e-host / manual check |
| **G6** | MemoryDenyWriteExecute | e2e-host MDWE probe on management-ui |
| **G7** | Optional hybrid TLS | `just check-tls-hybrid` after B1 (never CI live) |
| **G8** | VW ADMIN_TOKEN present (**S7b**) | kind `vaultwarden-admin` in staging; inventory with `--with-vaultwarden` |
| **G9** | VW enable fragment (**S7b**) | private host-local `surmount-vaultwarden-enable.nix` from cutover `--emit-fragments` |
| **G10** | VW unit smoke (**S7b**) | `systemctl is-active vaultwarden`; loopback curl; first admin UI once |

**Vaultwarden as human store (V4/S9):** after G10, keep reinstall copies
(Namecheap API notes, Stalwart admin notes, recovery) in VW. Full one-pager
below. Depth: [SECRETS.md](SECRETS.md) sections 5.1 and 5.2. S7a module is
done; do not rebuild it. S7b live unit remains operator host residual until
G8-G10 green on a real box. Do **not** invent Bitwarden Secrets Manager
(`bws`).

### Vaultwarden human path (one-pager; no boot pull)

**Plain English:** Vaultwarden is Domain **C**, a **human** store for copies of
deploy material (and day-to-day passwords). It is **not** free-443 activation
and **not** the thing that logs into Stalwart at cutover. Free-443, ACME hooks,
and host units read **Domain B files only**. Management-ui and free-443 do
**not** call Vaultwarden over the network at boot, and nothing in-tree pulls
VW during `nixos-rebuild` / activation.

```text
  Vaultwarden (Domain C)     human copies / recovery notes
           |
           |  optional: secrets-export-bw-to-staging  (or re-paste)
           |  preferred first fill: just secrets-prompt
           v
  Laptop staging / SS (A)    private Domain A (outside public git)
           |
           |  secrets-install-host / cutover install
           v
  VPS Domain B files         free-443 + ACME DNS hook + units read here
           |
           |  automatic ladder after A is filled:
           |  install -> free-443 -> prep -> dns -> deploy -> prove -> le-prod
           v
  free-443 / public HTTPS    Domain B credential must already be engine-accepted
```

| Store | Role | Feeds free-443 / ACME at cutover? |
|-------|------|-------------------------------------|
| **Vaultwarden (C)** | Human copies of deploy secrets; passwords, TOTP, notes | **No.** Never boot / activation SoT. |
| **Laptop Domain A** | Operator custody; source for install (`secrets-prompt` or export) | Holds values you install |
| **VPS Domain B** | Runtime files free-443 and ACME hooks read | **Yes.** |

### Fix public dashboard (full agent unlock; no password homework)

**Plain English:** When free-443 is stuck on HTTP 401 and there is no known
admin password, the agent can pin a **new** high-entropy recovery admin via
`STALWART_RECOVERY_ADMIN`, mint an API key, and free-443 dry-run. No operator
password paste required. Password never enters the Nix store or the systemd
drop-in body (drop-in only has `EnvironmentFile=-...`).

```bash
# Generate recovery pin, install Domain B + drop-in, restart, probe, mint API
# key, Domain B token install, free-443 dry-run (not live free-443 green):
just fix-public-dashboard -- --target root@HOST

# Reuse existing Domain A recovery staging:
just fix-public-dashboard -- --target root@HOST --reuse

# Live free-443 after dry-run is green (operator host only; never CI):
just fix-public-dashboard -- --target root@HOST --reuse --live-free-443
# or:
just free-stalwart-public-443 -- --live --restart
```

| Fact | Meaning |
|------|---------|
| Recovery driver | `just stalwart-recovery-unlock` / `nix run .#stalwart-recovery-unlock` |
| Compose | `just fix-public-dashboard` / `nix run .#surmount-fix-public-dashboard` |
| Order | **recovery -> Domain -> permanent Admin -> ApiKey -> free-443** (bootstrap ensures Domain + permanent Admin on empty directory before `create apikey`) |
| Kind | `stalwart-recovery-admin` -> durable `/var/lib/surmount/secrets/stalwart/recovery.env` |
| Env shape | `STALWART_RECOVERY_ADMIN=admin:<password>` (single line EnvironmentFile) |
| Load path preference | (1) unit `EnvironmentFile` from module `services.stalwart.recoveryAdminEnvFile` (path only) (2) durable `/etc/systemd/system/stalwart-mail.service.d/recovery-admin.conf` (3) runtime `/run/systemd/...` last resort |
| Drop-in body | `EnvironmentFile=-...` path only; never the password; never in Nix/`extraEnvironment` |
| Entropy | 32 alnum from `/dev/urandom` (openssl fallback) |
| free-443 green | Only after engine accept + free-443 live; compose default is dry-run |
| Hermetic | crate `surmount-stalwart-ops` tests in `checks.*.ci` |

Upstream recovery admin: [Recovery mode](https://stalw.art/docs/configuration/recovery-mode/)
(accessed: 2026-08-11). Dual-pin: [SECRETS.md](SECRETS.md).

**Hygiene (preferred):** after a permanent API key works, strip the recovery pin:

```bash
# Plan only:
just stalwart-recovery-unlock -- --strip --dry-run --target root@HOST
# Live strip (drop-in + recovery.env + restart):
just stalwart-recovery-unlock -- --strip --target root@HOST
# Or compose after mint (still strips if free-443 is BLOCKED after mint):
just fix-public-dashboard -- --target root@HOST --strip-recovery
```

Strip is **host hygiene only**:

- Removes durable + runtime drop-ins and Domain B `recovery.env`.
- Does **not** wipe laptop Domain A staging (`stalwart-recovery-admin`). Keep
  Domain A custody offline; shred Domain A yourself if the pin is fully
  retired (`reuse --install` can re-materialize Domain B from staging).
- Does **not** clear `services.stalwart.recoveryAdminEnvFile` in the host
  profile. If that option stays set, the unit still wires
  `EnvironmentFile=-path` (soft `-` so missing file does not fail start).
  Any later rewrite of Domain B at that path reloads recovery on next unit
  start without a new drop-in. Clear the option on the next rebuild after
  strip when you want the capability gone.

**Recommended default (tree, 2026-08-12):** `modules/mail.nix` sets
`recoveryAdminEnvFile` to the durable Domain B path
`/var/lib/surmount/secrets/stalwart/recovery.env` (`mkDefault`). The unit
wires `EnvironmentFile=-path` (missing file after strip does not fail
start). Password stays only in Domain B `recovery.env` (mode 0600), never
in Nix. Unlock skips writing a drop-in when the unit already lists that
EnvironmentFile. Opt out in private host-local with
`services.stalwart.recoveryAdminEnvFile = "";`. Live host still needs the
file present if you want unlock to survive reboot (Track C). Do **not**
leave recovery only on `/run`.

**Operator reboot + post-reboot proof (you reboot; agents do not):**

```bash
# After Track A/B/C files are on the host. Agent must not run this.
ssh root@YOUR_HOST reboot

# After the host is back (swap names for your zone; no secret dumps):
curl -sf https://services.surmount.systems/health
curl -sI https://services.surmount.systems/          # expect 307 /login
curl -s https://surmount.systems/ | grep -F 'Surmount Systems'
curl -s https://www.surmount.systems/ | grep -F 'Surmount Systems'
# Current GitHub site copy (live 2026-08-19 tip 1c84696; not UNDER CONSTRUCTION):
curl -s https://surmount.systems/ | grep -F 'BIP 360 + P2MR'
curl -s https://surmount.systems/ | grep -F 'Grok OSS'
# Fallback only if the document root has no index.html:
# curl -s https://surmount.systems/ | grep -F 'UNDER CONSTRUCTION'
# After Track B live:
curl -sf https://mta-sts.surmount.systems/.well-known/mta-sts.txt
# PEM + recovery path-exist only (modes/owners; never cat the files):
ssh root@YOUR_HOST 'stat -c "%a %U %n" \
  /var/lib/surmount/secrets/tls/cert.pem \
  /var/lib/surmount/secrets/tls/key.pem \
  /var/lib/surmount/secrets/stalwart/recovery.env'
```

Lower-level steps (same tools under the hood):

```bash
just stalwart-recovery-unlock -- --generate --install --target root@HOST
# password-only file derived from staging secret (0600), then:
just bootstrap-stalwart-token -- --password-file PATH --target root@HOST --free-443-dry-run
```

### Bootstrap Stalwart API token (preferred when admin password is known)

**Plain English:** Once you have the **first admin password** (bootstrap log,
recovery pin, or known login), you do **not** need the Stalwart WebUI to mint a
free-443 API key. Prefer the CLI path below. WebUI remains valid and optional.
When the password is **unknown**, prefer **Fix public dashboard** (recovery pin
generate) above.

```bash
# Mint API key via Basic auth, stage Domain A, install Domain B on the VPS
# (create runs on the host against loopback :8080 when --target is set):
just bootstrap-stalwart-token -- --password-file ~/.config/surmount/admin-pass \
  --target root@HOST

# Same + free-443 dry-run plan (not live free-443 green):
just bootstrap-stalwart-token -- --password-file ~/.config/surmount/admin-pass \
  --target root@HOST --free-443-dry-run

# Password from env (prefer file on shared hosts) or TTY no-echo prompt:
STALWART_PASSWORD='...' just bootstrap-stalwart-token -- --no-install
just bootstrap-stalwart-token -- --password-file ~/.config/surmount/admin-pass --no-install
```

| Fact | Meaning |
|------|---------|
| Auth | Basic (`--user` default `admin` / `STALWART_USER` + password) |
| Ensure directory | Before `create apikey`: ensure mail **Domain** (`--domain` / `STALWART_DOMAIN`, default `surmount.systems`) and **permanent Admin Account** (Admin role + Password; same password as intake). Idempotent when already present. Required on empty directory (recovery alone cannot mint ApiKey). Escape hatch: `--skip-ensure-directory` (known-warm re-mint only; fails on empty directory; keeps bare `--user` for mint). |
| Create | `stalwart-cli create apikey` with Inherit permissions (one-time `Secret:` line). After ensure proves permanent Admin exists (created or already present), mint uses `admin@domain` so the key attaches to directory Admin, not recovery bare `admin`. Permanent Admin is full engine Admin; ApiKey Inherit is maximal bootstrap privilege (rotate/scoped keys later residual). |
| Custody | Stages kind `stalwart-token`; reuses `add-stalwart-token` / secrets-install |
| Password | `--password-file`, `STALWART_PASSWORD`, or TTY; **fail closed if empty**. Permanent Admin password credential goes on create `--stdin` JSON (not on CLI argv). |
| WebUI | **Optional** once password is known; this CLI path is preferred |
| free-443 green | Only after engine accept + free-443 driver success; dry-run is not live green |
| Hermetic self-test | crate `surmount-stalwart-ops` in `checks.*.ci` (includes empty-directory ensure path) |

**Compose order** (with recovery unlock / fix-public-dashboard):

`recovery -> Domain -> permanent Admin -> ApiKey -> free-443`

Upstream create docs: [Creating objects](https://stalw.art/docs/management/cli/create/)
and [Account](https://stalw.art/docs/ref/object/account/) (accessed: 2026-08-11).
Dual-pin: [SECRETS.md](SECRETS.md) bootstrap honesty.

### Add Stalwart admin token (Domain A -> B)

**Copy-paste ready.** Use a credential **Stalwart already accepts** (first-boot
admin password or an API token the engine already knows). Do **not** invent a
random token. Values are never printed.

When you already have an admin password and need a **new** API key principal,
prefer **`just bootstrap-stalwart-token`** (section above) over WebUI paste.
Use `add-stalwart-token` when you already hold the API key string (or after
bootstrap staged it and you only need reinstall).

```bash
# 1) Laptop: interactive no-echo paste into Domain A (default host=surmount-1)
just add-stalwart-token
just add-stalwart-token -- --host surmount-1

# 2) Prefer pipe or file (no interactive prompt), then install Domain B
printf '%s\n' 'REAL_TOKEN' | just add-stalwart-token -- --host surmount-1 --target root@HOST
just add-stalwart-token -- --token-file ~/.config/surmount/stalwart-token \
  --host surmount-1 --target root@HOST

# 3) Staging already filled: --target installs Domain B without re-prompt
just add-stalwart-token -- --host surmount-1 --target root@HOST --dry-run-install
just add-stalwart-token -- --host surmount-1 --target root@HOST
# explicit install-only (same default when staging exists + --target):
just add-stalwart-token -- --host surmount-1 --target root@HOST --install-only
# force replace Domain A then install:
just add-stalwart-token -- --host surmount-1 --target root@HOST --replace

# 4) Free public :443 (needs the engine-accepted value in Domain B)
just free-stalwart-public-443 -- --dry-run
# live on operator host only (never CI green):
just free-stalwart-public-443 -- --live --restart
```

Equivalent lower-level path (same tools under the hood):

```bash
just secrets-prompt -- stalwart-token --host surmount-1
just secrets-install-host -- --from-staging "$HOME/.local/share/surmount/staging" \
  --host-id surmount-1 --target root@YOUR_HOST --require-kind stalwart-token
```

| Fact | Meaning |
|------|---------|
| Destination on VPS | Durable `/var/lib/surmount/secrets/ui/stalwart-api-token` (S8) |
| Existing Domain A + `--target` | **No re-prompt.** Installs existing staging (`using existing Domain A staging`). Use `--replace` / `--force-reprompt` to re-fill. |
| Pipe / `--token-file` | Non-interactive Domain A fill. Token-file must not be world-readable (prefer `chmod 0600`). |
| Missing credential + `--target` | Fails with short pipe/`--token-file` help (no hang on empty non-TTY). |
| generate / random invent | **Refused** for this kind as free-443-ready. Not engine registration. |
| Live free-443 **401** | Wrong/unknown admin value in Domain B. Fix the credential, not the script. |
| Hermetic self-test | crate `surmount-stalwart-ops` in `checks.*.ci` |

**How material gets into Domain A (pick one; never chat/shell history):**

| Path | When | What you run |
|------|------|--------------|
| **fix-public-dashboard (password unknown)** | No known admin password; need full unlock | `just fix-public-dashboard -- --target root@HOST` (urandom recovery pin + mint + free-443 dry-run). |
| **bootstrap-stalwart-token (preferred when password known)** | Have first admin password; need engine-minted API key | `just bootstrap-stalwart-token -- --password-file ... [--target HOST]`. WebUI optional. |
| **add-stalwart-token** | Already hold an engine-accepted API key string | Pipe, `--token-file`, or interactive paste. Optional `--target` installs Domain B. Existing staging + `--target` skips re-prompt. Defaults host `surmount-1`. |
| **secrets-prompt (any kind)** | Real value is in your head, paper, or another tool | `just secrets-prompt -- stalwart-token --host YOUR_LOGICAL_HOST` (and other kinds: `session-secret`, `namecheap-api`, `shc-api`, ...). No-echo paste into private staging. |
| **VW then export** | Value already lives in Vaultwarden (after S7b, or you stored it there first) | Unlock official `bw` against your VW URL; run `just secrets-export-bw-to-staging -- --map ... --staging ... --host-id ... --from-bw` (map has labels only, no secret bytes). Or manual `bw get` into staging layout ([SECRETS.md](SECRETS.md) 5.2). |
| **Generate (session / VW admin only)** | Non-engine kinds | `--generate` / cutover `--generate-material`. **Never** treat generated `stalwart-token` as free-443-ready (engine registration is separate). |

**After Domain A is filled (automatic path; not VW at boot):**

```bash
# 1) Install Domain B from private staging (dry-run first)
just cutover-step install -- --dry-run \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --acme-path
# live when ready:
just cutover-step install -- --live \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --acme-path

# 2) Free Stalwart public :443 (needs Domain B stalwart-token Stalwart already accepts)
just cutover-step free-443 -- --target root@YOUR_HOST --dry-run
# live when ready (operator machine only; never CI green):
just cutover-step free-443 -- --target root@YOUR_HOST --live

# 3) Continue ladder: prep -> dns -> deploy -> prove -> le-prod
# (see Laptop-driven cutover ladder below)
```

Or one compose after staging is full (still **not** a VW boot pull):

```bash
just host-cutover -- --live \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --host-local /path/to/private/host-local \
  --acme-path --host-profile /path/to/private/host-profile.toml \
  --free-443
```

**Explicit "not at boot" warning:**

| Forbidden | Why |
|-----------|-----|
| management-ui or free-443 HTTP to Vaultwarden at boot | Chicken-egg; VW may be down; activation must not depend on C |
| systemd / `nixos-rebuild` calling `bw` or VW API | Purity + chicken-egg; no in-tree C->B pull |
| Treating "I put it in VW" as "free-443 will work" | free-443 reads Domain B only; install first |
| Live VW / live `bw login` as CI green | Hermetic fixture/mock tests only (crate `surmount-secrets-install`) |
| Bitwarden Secrets Manager (`bws`) | VW is password-manager API only; **Q-SEC-SM-*** open |

**S7b vs human copies:** enabling the VW **unit** (G8-G10, `--with-vaultwarden`)
is separate from using VW as a **human store**. First cutover can (and often
should) fill Domain A with `secrets-prompt` and free-443 **before** S7b.
After G10, put reinstall copies into VW for comfort; export back to A when
you reinstall. Detail: [SECRETS.md](SECRETS.md) 5.1 / 5.2.

Machine-readable gate ids: [host-cutover-gates.txt](host-cutover-gates.txt).

```bash
# Dry-run plan (default; no remote mutation). Staging outside public git.
just host-cutover -- --dry-run \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --host-local /path/to/private/host-local

# https + Vaultwarden (S7b) profile
just host-cutover -- --dry-run --with-vaultwarden \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --host-local /path/to/private/host-local \
  --emit-fragments /path/to/private/host-local

# Optional: generate non-CA material into private staging (session, tokens, VW)
just host-cutover -- --dry-run --staging /path/staging --host-id mail-lab \
  --generate-material --acme-path --with-vaultwarden

# Live install + switch (operator machine only; never CI green)
just host-cutover -- --live --staging /path/staging --host-id mail-lab \
  --target root@YOUR_HOST --host-local /path/host-local --with-vaultwarden

# Hermetic self-test (GHA quality; no network)
nix build .#checks.$(nix eval --impure --raw --expr 'builtins.currentSystem').surmount-host-cutover-test
```

Profiles: default **https-only** (PEM path requires `tls-cert` + `tls-key` +
session + stalwart token). **`--with-vaultwarden`** adds `vaultwarden-admin`.
**`--acme-path`** makes PEMs optional (ACME staging + external-hook path) and
ensures Domain B **writable parents** (`tls/` + `acme/`) via
`secrets-install-host --ensure-acme-parents` when a destination is set.
**No operator CA PEMs required** for that path. Lab DNS-01 hook sample
(temp zone file, not production): `nix run .#acme-dns-hook-lab`. Production
hooks talk to **operator-owned** DNS.

When staging already has **`namecheap-api`**, cutover adds it to
`--require-kind` (never invents Namecheap credentials). **`--host-profile
PATH`** renders non-secret `host-local-acme.nix` into **`--host-local`** via
`render-host-profile-acme` (always `--force` on re-run). **`--free-443`**
plans/runs the free-Stalwart-:443 driver after secrets-install.

### Laptop-driven cutover ladder (P0 offline ship)

Run from the **office laptop** (long-term secret custody SoT). The VPS receives
**Domain B activation copies** when you run **install**: prefer durable paths
under `/var/lib/surmount/secrets` (TLS PEMs + ACME account H-PEM default,
session, Stalwart token, Namecheap env) so they **survive reboot**. Ephemeral
`/run/surmount-secrets` remains allowlisted as an operator override. Dry-run is the default. Each
step prints what it will do, what it will **not** claim (no "HTTPS done", no
"LE issued", no "engine token registered"), and the exact next command.

| Step | What | Typical command |
|------|------|-----------------|
| **material** | Inventory private staging; optional `--generate-material` for non-CA kinds | `just cutover-step material -- --staging PATH --host-id ID --acme-path` |
| **prep** | Render private host profile (ACME + public listen) into host-local | `just cutover-step prep -- --host-profile PATH --host-local DIR` |
| **install** | Laptop → VPS durable Domain B secrets (default) + ACME parents | `just cutover-step install -- --staging PATH --host-id ID --target HOST --acme-path` |
| **free-443** | Free Stalwart public :443 **on the VPS via SSH** when `--target` is set | `just cutover-step free-443 -- --target HOST` |
| **dns** | Plan/apply A/AAAA from profile domains (laptop Namecheap; dry-run default) | `just cutover-step dns -- --host-profile PATH --dns-a IPV4` |
| **deploy** | `deploy-host` switch + loopback smoke | `just cutover-step deploy -- --target HOST --host-local DIR` |
| **prove** | Checklist + optional `just e2e-host` when `BASE_URL` set (never soft-elevates) | `just cutover-step prove` |
| **le-prod** | Explicit flip render to production Let's Encrypt directory; re-deploy after | `just cutover-step le-prod -- --host-profile PATH --host-local DIR` |

```bash
# Full dry compose of safe planning steps
just host-cutover -- --step all --dry-run \
  --staging /path/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --host-local /path/private/host-local \
  --acme-path --host-profile /path/private/host-profile.toml
```

**Let's Encrypt vs cert files on the VPS (plain English):**

1. **Let's Encrypt** is the public certificate authority (who signs the cert).
   Production public HTTPS uses the **Let's Encrypt production** directory
   (browser-trusted). Staging is a temporary test directory only.
2. **Live path (laptop custody):** ACME DNS-01 runs on the **laptop**.
   Namecheap `ClientIp` is **laptop egress**, not the VPS. Named recipe:
   `just laptop-renew-cert -- --directory production` (directory required;
   never silent staging/prod). Host in-process ACME stays **off**.
3. After issue, install durable PEMs to the VPS
   (`/var/lib/surmount/secrets/tls/{cert,key}.pem`) via
   `secrets-install-host` (kinds `tls-cert` + `tls-key` only) and restart
   the UI. That is how the box speaks HTTPS.
4. Host ACME (`acme.enable = true` + Domain B `namecheap.env`) is **not**
   the live path. Do not flip it on as a renew shortcut. VPS Namecheap
   whitelist is only required if a process on the VPS hits Namecheap.
5. Start with **staging** LE only for a first practice issue (browser trust
   is fake). Production needs an explicit `--directory production` (H6).
   Certificate hostnames come from the private host profile (`acme_domains`).
   Sample fixture lists services + mail + apex + www. **Live leaf (2026-08-18)**
   covers **services, apex, www, mta-sts, and mail** (five-name production
   leaf; same pair on :443 and :993). Do **not** re-issue just to add a
   name that is already on that leaf.

**Do not** run live zone rewrites while ACME TXT challenges are in flight.
**Do not** treat live LE or live Namecheap as CI green.

### Named laptop renew (`just laptop-renew-cert`)

This is the production renew path. Do not put a calendar reminder in place
of it. Host in-process ACME stays **off** (`SURMOUNT_ACME_ENABLE=0`).
Namecheap DNS-01 stays on the **laptop** (`ClientIp` = laptop egress).
The same Let's Encrypt production leaf covers **services, apex, www,
mta-sts, and mail**. TLS key on the host is **0600**
`surmount-ui` (owner-only). Stalwart uses copies under
`/var/lib/surmount/secrets/mail/tls/` (key 0600 `stalwart-mail`).

`--directory` is required (never silent staging or production). Namecheap
env is required and must be a regular file mode 0600 (symlink refused).
Production `--issue` requires `SettleSeconds` at least 120 in that env.
Hermetic: crate `surmount-acme-namecheap` in `checks.*.ci`.

```bash
# Safe: inspect staged PEMs. Never contacts Let's Encrypt.
just laptop-renew-cert -- --check --directory production

# Operator live: issue only if the leaf is missing or inside the 30-day
# window. Otherwise restage the matching pair, install tls-cert + tls-key,
# restart the UI, prove health 200 and IMAP/SMTPS Let's Encrypt.
just laptop-renew-cert -- --live --directory production \
  --host-profile ~/.local/share/surmount/host-profile.toml \
  --target root@YOUR_HOST

# Laptop user timer (not a VPS ACME timer). --target is required so weekly
# --live can install PEMs. Then enable the timer on the laptop:
just laptop-renew-cert -- --install-timer --directory production \
  --host-profile ~/.local/share/surmount/host-profile.toml \
  --target root@YOUR_HOST
systemctl --user daemon-reload
systemctl --user enable --now surmount-laptop-renew-cert.timer
# Example units: script/laptop-renew/
```

`--live` does **not** detect missing certificate hostnames (expiry only).
When adding extra static site FQDNs (apex + www for each proven site),
extend private `acme_domains` then **`--issue`**:

```bash
just laptop-renew-cert -- --issue --directory production \
  --host-profile ~/.local/share/surmount/host-profile.toml \
  --install --restart-ui --target USER@HOST
```

Do not start host in-process ACME. Do not run `--issue` concurrent with
a Namecheap zone rewrite.

If Namecheap returns Invalid request IP: **BLOCKED**. Whitelist **laptop
egress**. Do not demand a VPS whitelist for this path.

Populate extra static roots (after AFP mount of DS3018xs `sites`):

```bash
just sync-static-sites-from-ds3018xs -- --live --target USER@HOST
```

**Live Track B (2026-08-12):** private profile + production leaf already
include `mta-sts.<domain>`; host-local `managementUi.mtaStsMode = "testing"`
is on; public policy body is **200** at
`https://mta-sts.<domain>/.well-known/mta-sts.txt`. Stay **testing**.
**Primary public MX is this host (2026-08-20):**
`10 mail.surmount.systems`; EmailType **MX**. `--live set-mx` is still
**not** a public MX flip while Email Forwarding is on (EmailType FWD
fails closed; extra domains). `list` prints EmailType. Do **not**
re-issue just to add mta-sts or mail; both are already on the
production leaf. Do not `enforce` until the operator asks.
Report: `.agents/reports/impl-earn-trust-wave.md`.

### ACME-live cutover recipe (V5 one-pager)

Order of operations once **Domain A** (private staging or Secret Service) has
values. Paths below are placeholders only; never commit real host IPs, keys,
or tokens. Prefer dry-run first; live only on the operator machine. Prefer the
**laptop ladder** above when driving step-by-step.

| Step | What | Command / note |
|------|------|----------------|
| **0** | Prerequisites | Private staging **outside** public git; private host-local; private host profile (`acme_domains` + `acme_email`); SSH target; second session for switch hang |
| **1** | Domain A material | Put `session-secret`, `stalwart-token`, and (for DNS-01) `namecheap-api` into private staging (or Secret Service). **Prefer** `just secrets-prompt` on the **operator laptop** (no-echo paste; never chat/shell history). Optional if values already live in Vaultwarden: `secrets-export-bw-to-staging` into the same private staging (Domain C human store only; **no boot pull**). See *Vaultwarden human path* one-pager. For `stalwart-token`, paste a credential **Stalwart already accepts** (`just secrets-prompt -- stalwart-token`); random generate is **not** engine registration (see free-443 callout; live **401** = wrong/unknown admin). Namecheap `ClientIp` is the **API caller** public IP. Laptop DNS-01 (the live path) needs **laptop egress** on the Namecheap whitelist, not the VPS. Optional: cutover `--generate-material` for **session** (+ VW admin when S7b); do **not** treat generated `stalwart-token` as free-443-ready. **Never** generates Namecheap API or PEMs |
| **2** | Non-secret edge config | Fill private host profile (copy from `script/testdata/host-profile/sample-host-profile.toml`). Staging LE directory until proven |
| **3** | Day-1 A/AAAA | Registrar UI or `just dns-zone-namecheap` (not concurrent with ACME TXT challenge). PTR at VPS provider (SHC: `just rdns-shc` when `shc-api` key present, or console) |
| **4** | Dry-run compose | One recipe (below). Confirm require-kinds, ensure-acme-parents, profile render, free-443 plan. No remote mutation |
| **5** | Live compose | Same flags with `--live` (needs `--target` + `--host-local`). Installs Domain B secrets + ACME parents; renders profile; optional free-443 |
| **6** | Deploy switch | Cutover without `--skip-deploy` runs `deploy-host` (or run deploy separately after secrets). Post-switch smoke always runs |
| **7** | Prove staging LE | Units up; free :443 if not already; hook on host; multi-name staging when profile lists services+mail+apex+www (H5); `just e2e-host` when `BASE_URL` set. Then **explicit** production LE only: `just laptop-renew-cert -- --directory production` (preferred laptop path) **or** re-render with `--directory production` (profile file stays staging) **or** set profile `acme_directory` to the production URL and re-render (H6; no silent staging->prod). Redeploy + re-issue |
| **8** | Optional S7b | `--with-vaultwarden` after HTTPS path is sane; G8-G10; then human copies into VW (Domain C) |

```bash
# --- Fill Domain A (prefer no-echo CLI; outside public git) ---
# just secrets-prompt -- session-secret --host YOUR_LOGICAL_HOST --generate
# just secrets-prompt -- stalwart-token --host YOUR_LOGICAL_HOST
# just secrets-prompt -- namecheap-api --host YOUR_LOGICAL_HOST
# Default staging: ~/.local/share/surmount/staging (or $XDG_DATA_HOME/...)
#
# --- Once Domain A staging is filled (outside public git) ---
# Staging layout (kinds): session-secret, stalwart-token, optional namecheap-api
# Profile: ~/.local/share/surmount/host-profile.toml (non-secrets only)
# Host-local: private overlay for SSH keys + rendered host-local-acme.nix

# Dry-run (default; hermetic-friendly with --dest-root instead of --target)
just host-cutover -- --dry-run \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --host-local /path/to/private/host-local \
  --acme-path \
  --host-profile /path/to/private/host-profile.toml \
  --free-443

# Same with optional Vaultwarden enable path (S7b material must exist)
just host-cutover -- --dry-run \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --host-local /path/to/private/host-local \
  --acme-path --host-profile /path/to/private/host-profile.toml \
  --free-443 --with-vaultwarden --emit-fragments /path/to/private/host-local

# Live install + deploy (operator machine only; never CI green)
just host-cutover -- --live \
  --staging /path/to/private-staging --host-id YOUR_LOGICAL_HOST \
  --target root@YOUR_HOST --host-local /path/to/private/host-local \
  --acme-path \
  --host-profile /path/to/private/host-profile.toml \
  --free-443
```

**What that one recipe does (ordered):**

1. **G1 inventory** under `--acme-path` (PEMs optional; session + stalwart required; namecheap when staged)
2. **Render** `host-local-acme.nix` from `--host-profile` into `--host-local` (non-secret; `--force`)
3. **G2 secrets-install** for present require-kinds (`session-secret`, `stalwart-token`, optional `namecheap-api`, optional `vaultwarden-admin`)
4. **Ensure ACME parents** on Domain B (`tls/` + `acme/`; no PEM payloads)
5. **Deploy** (unless `--skip-deploy`) with host-local overlay including the rendered ACME fragment
6. **Optional free-443** when `--free-443` (dry-run of free driver unless cutover `--live`; BLOCKED if token missing is a note in dry-run)

**Still operator one-time (automation does not invent these):** Namecheap API
key, first Stalwart admin if the engine does not yet accept the token file,
edge hostname choice, LE contact email, Day-1 A/AAAA (or zone tool).

Hermetic: crate `surmount-host-cutover` in `checks.*.ci` (includes V5 dry-run compose text contracts).

### Secrets install (domain A -> B) before or beside cutover

Operator custody prefers OS Secret Service / private staging on the
**workstation** (domain A). Host units still need **files on the box**
(domain B). Install bridge (S0-S3 offline; not live CI):

```bash
# ACME Domain B parents only (no PEMs / no account payload). L2 prep.
nix run .#secrets-install-host -- --ensure-acme-parents --target root@YOUR_HOST

# Staging dir (primary; headless-friendly). Never from public git paths.
nix run .#secrets-install-host -- --from-staging /path/to/private-staging \
  --host-id YOUR_LOGICAL_HOST --target root@YOUR_HOST \
  --require-kind tls-cert --require-kind tls-key
# or: just secrets-install-host -- --from-staging ... --host-id ... --target ...
# Dry-run / hermetic practice: --dest-root /tmp/fake-root (no SSH)
```

Schema, inventory, runbook: [SECRETS.md](SECRETS.md) section 1. Hermetic
crate tests: `nix build .#checks.<system>.surmount-secrets-install-test`.
**Never** green CI against a live keyring or live host install.

**Opt-in S4 (offline shipped):** `nix run .#surmount-deploy-host -- --install-secrets`
with `--secrets-staging DIR` (or `--secrets-from-secret-service`) and
`--secrets-host-id ID` runs the install bridge **before** rebuild. Default
deploy without that flag remains pure sync+rebuild (no auto-install). Env
equivalent: `SURMOUNT_DEPLOY_INSTALL_SECRETS=1`. Self-test covers default-off
and opt-in dry-run in crate `surmount-deploy-host`.

### Host-local overlay + deploy automation

Private box material (hardware-config, SSH authorized keys, real hostname,
static net, PEMs, age keys) lives in a **host-local** directory on the machine
or a private operator path. It never lands in public git. Contract and example
layout (placeholders only): [deploy-host-local.md](deploy-host-local.md).
Prefer Secret Service / install bridge for **secret file** material (TLS PEMs,
session secret, tokens); host-local remains the Nix/SSH/network overlay.

Repeatable operator deploy (not CI switch):

```bash
nix run .#surmount-deploy-host -- --dry-run --target root@YOUR_HOST \
  --host-local /path/to/private/host-local
# or: just deploy-host --dry-run --target root@YOUR_HOST --host-local ...
# (optional: just deploy-host -- --dry-run ... if just swallows flags)

# Opt-in: install domain B secrets then rebuild (S4)
nix run .#surmount-deploy-host -- --dry-run --target root@YOUR_HOST \
  --host-local /path/to/private/host-local \
  --install-secrets --secrets-staging /path/to/private-staging \
  --secrets-host-id YOUR_LOGICAL_HOST \
  --secrets-require-kind tls-cert --secrets-require-kind tls-key
```

The driver requires an operator-supplied target (no committed real host;
rejects option-shaped targets starting with `-`), checks host-local for
usable SSH key **files** (lockout risk if password auth is off), syncs the
**public** tree (refuses copying host-local into tracked `hosts/` /
`secrets/`), rsyncs private host-local to `REMOTE/host-local/`, optionally
runs the secrets install bridge when `--install-secrets` is set, runs
`nixos-rebuild switch --flake path:REMOTE_DIR#mail-vps` on the host (or prints
that plan in `--dry-run`), then **always** runs post-switch smoke on the remote
(`nix run .#surmount-deploy-host-post-switch-smoke`) even if rebuild exit was non-zero
(activation may still have applied). Smoke checks generation current, unit
activity (`sshd`, `stalwart-mail`, `surmount-management-ui`; starts UI if
inactive), loopback `/health` (never invents a public URL), TLS PEM path
presence when https edge or `SURMOUNT_TLS_*` is set (durable default
`/var/lib/surmount/secrets/tls/{cert,key}.pem`; paths only, never PEM
contents), optional ACME enable note (`SURMOUNT_ACME_ENABLE`), and when
https edge is configured, `ss` listen proof for **:443** and **:80**. The
`path:` form makes untracked host-local visible to Nix; `flake.nix`
auto-imports `./host-local` when present (known names + authorized_keys).
Contract: [deploy-host-local.md](deploy-host-local.md). Self-tests:
crate `surmount-deploy-host` (also `checks.*.ci`).

**B0 honesty:** a passing driver key-file check is **necessary but not
sufficient** alone. With the known-name host-local layout, flake auto-import
wires `authorized_keys` into evaluated `keyFiles` on path: rebuild; a custom
`host-local/default.nix` must wire keys itself. Public rsync excludes are
hygiene helpers only; keep using `surmount-private-data`. Host-local
remote rsync defaults **without** `--delete` (opt in with
`--host-local-delete`).

Security elevation after first boot is a **host ladder** (B0 access preserve
through B7 fail2ban shrink). Stages and proof gates:
[deploy-host-local.md](deploy-host-local.md) section 6. Do not enable
requireDeployMaterial / public https / Arti / ban on the sample host in git
without host material.

If `nixos-rebuild switch` appears to hang on **reloading user units for
root**, that hang is a **known ops residual**. Prefer a **second SSH session**
before risky switches. After the switch returns (or after hang recovery),
verify generation + units via deploy-host post-switch smoke (or re-run
`nix run .#surmount-deploy-host-post-switch-smoke` on the host). Detail: activation
reliability in [deploy-host-local.md](deploy-host-local.md) section 5
(measured workaround, not a product claim).

### Host process view (`just btop`)

`just btop` (`nix run .#surmount-btop-host`) uses Eternal Terminal to the same
target as `just deploy-host` and `just et`, then runs interactive `btop`
(`et -c btop`). It does not use a raw SSH that can freeze-paint a last frame.
After btop exits, the Eternal Terminal session exits (not `et --noexit`).
Needs `etserver` on the guest (after `just deploy-host`). Target comes from
`--target`, `SURMOUNT_DEPLOY_TARGET`, or
`~/.local/share/surmount/agent-target.env`. Fails loud if those are missing,
if `et` is not on PATH, or if stdin and stdout are not a live tty (a frozen
last-paint pane is not live). If you are already on the mail guest (Eternal
Terminal, guest hostname, or `/etc/surmount/root-justfile`), the wrapper runs
local `btop` and refuses nested `et`. Guest `/root/justfile` `just btop` is
local `btop`. If the clock in that pane stops, the pane is dead: press q,
exit the remote shell, then run `just btop` from the laptop.
Hermetic: crate `surmount-host-probe`.
Host journal follow is `just host-logs` (same target; no sudo).
Do not put provisioned host addresses in the public tree.
Host btop uses the operator local theme and settings (flat-remix, no theme
background, block graphs, square corners). Canonical file is
`/etc/xdg/btop/btop.conf`. Activation also links
`/root/.config/btop/btop.conf` (and the same path under any defined
normal user's home) so `just btop` loads it without extra flags.

### Host hardware probe (`just host-inxi`)

Measure the **mail host** with `inxi` (or `lscpu`) **on the mail host**.
Laptop `inxi` is a different machine. Do not copy laptop cores onto
`modules/remote-builder.nix` or into the machines-file slot field.
`inxi` on the guest must **not** need sudo.

`just host-inxi` (`nix run .#surmount-inxi-host`) SSHes to the same target as
`just deploy-host` / `just btop` and runs `inxi` (default `-Fxxxz -c0`).
Key-only (`BatchMode`, publickey). No TTY. No sudo. `--userland` runs
`nix shell nixpkgs#inxi -c inxi ...` so a login without the host
package (for example `nixbuilder` before the next deploy) can still
read specs. After a `deploy-host` switch, `modules/inxi.nix` puts
`pkgs.inxi` on PATH so `ssh surmount-1 inxi -C` works without sudo.
Target comes from `--target`, `SURMOUNT_DEPLOY_TARGET`, or
`~/.local/share/surmount/agent-target.env` (override path:
`SURMOUNT_AGENT_TARGET_ENV`). Fails loud if those are missing or if
`ssh` is not on PATH. Hermetic: crate `surmount-host-probe`. Targeted eval:
`just test-inxi-eval`. Do not put provisioned host addresses, SKUs, or
plan names in the public tree. Prefer not to paste RAM/swap sizes into
product docs.

### Remote Nix builder (operator 2026-08-17; Lake default-off 2026-08-25)

**surmount-1** is the allowed remote Nix builder (ssh-ng). Mail and builder
work share that host. That is why niceness and a hard memory cap both
matter. This is **not** NixOps.

**Lake (2026-08-25):** `surmount.lake` is an **optional default-off** unit
(`modules/lake.nix`). Enable stays false unless private host-local turns
it on. When enabled it is niced (`Nice=19`, idle ionice), has a hard
`MemoryMax` on **`surmount-lake.service`** (the process that runs
`lake`, not only a user slice), journals start/stop/OOM/cgroup kill
lines, and uses `cpuQuota = auto` (95 percent of `onlineCpus` when that
is set from host-local; never `CPUQuota=95%` as one CPU). `jobs`
scaffold is a memory-safe slot count, not guest `nproc`. Do **not**
start Lake on the live guest from an agent. Uncapped Lake previously
OOM-killed systemd (console evidence 2026-08-18). A stray
`systemd.services.surmount-lake` overlay without `surmount.lake.enable`
still fails eval.

**Honesty (2026-08-18):** We started niced Lake on **surmount-1**
**without** systemd `MemoryMax`. Niceness is **not** a memory cap. That
is why the box went dark: Lean/Lake OOM killed systemd (console
evidence 2026-08-18), and the hypervisor OOM-shutdown the guest as a
protection step. SHC ticket 261 is **closed**. Do **not** open another
ticket. **Agents never reboot.**

**SHC tickets 261 / 262 / 263 (refresh 2026-08-25):** Ticket 261
(display 7785310) stays **closed**. Ticket 262 (display 5015746) is
now **closed**; staff merged it into 263. Ticket 263 (display 6078688)
is **on hold**, emergency. Staff (Dimi) restored the guest after
another host-side pressure event. They moved swap onto the compute
node, put zswap in front, added cgroup fences so the guest throttles
into swap instead of starving the hypervisor, added a hardware
watchdog, and added NOC alerts. Later the same day they said two of
five hypervisor-side changes did not stick after recovery, they
reapplied those with persistence, and they enabled a host watchdog
that should cut a repeat outage to about ten minutes. Guest-agent
talk on their side is waiting on the next reboot acknowledgement;
they have inject tools once the agent replies, and they said that is
not a silver bullet. Do **not** publish swap size, RAM, or SKUs.
Ticket 261 staff already asked for about-95-percent CPU, RAM, and
storage guards and noted NixOS had no working QEMU guest agent, so
they could not inject a command when SSH was stuck. **Agents never
reboot.**

**In-guest follow-up (2026-08-25):** Builder `MemoryMax` is a **builder
budget**, not 95 percent of the whole guest (that lets rustc starve
mail). `nix-daemon` gets a positive `OOMScoreAdjust` so it is the
preferred in-guest OOM victim; mail, management-ui, and sshd get a
negative score and stay un-niced. `surmount.hardening.qemuGuestAgent`
starts `services.qemuGuest` when enabled from host-local so staff can
inject a command next time. Disk refuse stays at 95 percent. `cpuQuota
= auto` stays 95 percent times online CPUs on the niced wrapper. Live
generation still has niced nix-daemon with a MemoryMax near whole
guest RAM, OOM scores at 0, and no qemu-guest-agent unit; those land
on the next operator NixOS switch. We replied on ticket 263 that the
guest is reachable, Stalwart is active, and guest agent waits on that
switch (not a reboot from us). Do not ping extra about Signal. Do not
reboot. Do not start Lake from an agent.

**In-tree (2026-08-18):** `surmount.remoteBuilder` (default **off**) installs
the `nixbuilder` user (key-only), `/etc/surmount/niced-builder`,
`/etc/surmount/niced-nix-daemon-stdio`, slice `surmount-builder.slice`,
`user-<uid>.slice`, and **`nix-daemon.service`** with systemd `MemoryMax`
(scaffold default `4G`, not a published guest RAM size). Trusted ssh-ng
forwards rustc to system nix-daemon; a user-slice-only cap does not
hold those workers. `cpuQuota` default is **`auto`**: no one-CPU
`CPUQuota=95%` on the slices (that parks cores and starves stdio).
The niced wrapper turns `auto` into 95 percent times **guest** `nproc`
(measure on the guest with `ssh surmount-1 inxi` / `lscpu`; do not paste
laptop `inxi` here). Guest `inxi` must **not** need sudo (`just host-inxi`
or `--userland` `nix shell nixpkgs#inxi`).
The operator laptop is **local**. **surmount-1** is the remote builder.
Never call the laptop remote. If the guest is down (snapshot, power
off, network), prefix any `just` recipe with `BUILD_LOCAL=true` so Nix
does not wait on ssh-ng. Example: `BUILD_LOCAL=true just rekey`. That
sets `NIX_CONFIG` `builders =` and `max-jobs = auto` for that command
only. Do not copy guest core counts onto the laptop. Pre-commit honors
the same variable. When the guest is up, omit it so rustc still goes
to the niced remote builder. `maxJobs` (scaffold default is a
memory-safe slot count, **not** laptop cores and **not** a published
guest core count) sets host `nix.settings.max-jobs`. That slot count
must stay under `memoryMax` so Nix does not spawn more parallel rustc
than the cgroup can hold. Using every guest CPU as a job slot is the
OOM path. `cpuQuota = auto` with `onlineCpus` unset leaves systemd
`CPUQuota` off (so we never pin one CPU). Set `onlineCpus` from
host-local after measuring the guest if you want a 95-percent-of-online
quota on `nix-daemon.service`. Do not copy laptop `nproc`. Before changing
that option, measure the mail host, then read **live** guest
`nix show-config` `max-jobs`. Laptop `~/.config/nix/machines` and
laptop `/etc/nix/machines` **max-jobs** must match that **guest** number.
Those files live on the laptop Nix **client** and describe surmount-1.
Do **not** copy laptop `nproc` or `inxi` onto `modules/remote-builder.nix`
or into the machines slot field. Laptop **local** `max-jobs` is a
different knob: measure the laptop with `inxi -C` (16 threads if
setting laptop system `max-jobs`). When enabled, the module also sets
`nix.settings.extra-system-features = [ "surmount-remote" ]` so the
builder **nix-daemon** accepts rustc jobs that require that feature
(client machines-file field 6 already advertises it). The client
machines-file `features` column must match the builder daemon
`system-features` (including `surmount-remote`). grok-build
`just check-remote` (`require_remote_builder`) gates on the remote
daemon listing `surmount-remote` in `system-features`. This repo's
`just check-remote` is thinner: `nix build .#checks.<system>.ci` with
`--option max-jobs 0` (laptop must not rustc). It prints that it is
running `nix build`, then `-L` plus `--print-out-paths` so a cache hit
still prints a store path instead of a blank prompt. No `--log-format
raw` (that hid derivation lines). No 20s heartbeat.
It does not pass `-v` (that prints every nixpkgs file) and does not pass
`--store ssh-ng`; the machines file plus max-jobs 0 is the force-remote
path. `just audit-remote` is the same for
cargo-audit. `BUILD_LOCAL=true` is the opposite (laptop build) and is
refused. A client advert
without the daemon feature schedules rustc, then the daemon refuses:
missing system features. `extra-` keeps NixOS auto-detected
`big-parallel`. Do not replace `system-features` with only
`surmount-remote`. After a module change, rebuild and switch this host
(`just deploy-host` or host `nixos-rebuild switch`) so
`/etc/nix/nix.conf` picks up the extra feature; then restart is
implied by the switch. Do not advertise a fake high slot count. Speed factor is
not a substitute for `MemoryMax`. Disk refuse at `diskGuardPercent`
(default 95). Enable from private host-local. Host-local may set a
real `memoryMax` as the **builder budget**; never publish that number
as a guest size in git. Do not set it to about 95 percent of the whole
guest while mail shares the box.
Mail and other critical units are **not** on this slice and do
**not** get this `MemoryMax` or `Nice=`. `nix-daemon` **does** get
`Nice=19` and idle ionice when the module is enabled (dev builds
only; not mail).

Hermetic tests: crate `surmount-niced-builder` in `checks.*.ci`, `just test-remote-builder-eval`,
`just test-logging-eval`, `nix eval --impure --json --file tests/lake-contract.nix`,
and `tests/module-eval.nix` t39* / t42* / t44*.
No live SSH. t39* prove the builder contract (user, trusted-users, slice
caps, nix-daemon MemoryMax/Nice, max-jobs, extra-system-features
`surmount-remote`, niced stdio, `onlineCpus` multi-CPU quota). t44*
prove Lake is default off and, when enabled, MemoryMax/Nice land on
`surmount-lake.service`. t39e still refuses a Lake overlay that is not
`surmount.lake.enable`.

**Live (2026-08-18):** private host-local sets `enable = true`, pins the
existing `nixbuilder` uid, drops the duplicate stdio wrapper, sets
`memoryMax` privately, `cpuQuota = "auto"`, and `maxJobs` to the
memory-safe slot count measured on the **guest** (not laptop inxi).
Laptop ssh-ng remote-program stays
`/etc/surmount/niced-nix-daemon-stdio`. Laptop **user**
`~/.config/nix/machines` **max-jobs** already matches live guest
`nix.settings.max-jobs` (that match is guest-sourced; it is not
laptop physical cores). Laptop **system** Nix does not read the user
file. `/etc/nix/machines` plus system `builders = @/etc/nix/machines`
still need a **local** sudo TTY on this laptop (agent `sudo -n` was
denied). That password prompt is laptop sudo, not a claim the laptop
is remote. Copy the user machines **template** (URI, key path,
features) and keep the slot field from the live guest. If also setting
laptop system `max-jobs`, use laptop `inxi` thread count (16), never
rewrite the machines slot from that 16. Do not start Lake from an
agent. Host-local `surmount.lake.enable = true` is the only start path,
and it is still default off.

Operator correction 2026-08-18: Nix is misconfigured if builds do not
make maximally nice use of cores without also OOMing the guest. That
is the contract. Do not retarget the question to static-site slowness.

Dev builds (remote Nix store builds, flake check, hygiene compiles) are
always maximally nice: `nice -n 19` and idle ionice when practical.
Prefer a niced builder user and a niced remote-build path. Prefer
build-on-laptop then switch so the host does not compile the UI. Do
**not** set `Nice=` on mail or other critical units (`stalwart-mail`,
`surmount-management-ui`, `sshd`, `surmount-arti-hidden-service` / Arti,
networking). Do **not** nice the whole machine. Starving mail to make a
compile cheaper is a miss.

**Niceness is not a memory cap.** We already started niced Lake without
`MemoryMax`; that is why the box went dark (guest OOM / hypervisor
protection). Builder work **must** have a hard memory cap (systemd
`MemoryMax` / cgroup `memory.max`) plus the about-95-percent CPU, RAM,
and storage guards SHC asked for so torture tests stay manageable. SHC
ticket 261 (closed 2026-08-18) said our RAM use at the tail end
cannibalized the hypervisor host, and Proxmox automatically
OOM-shutdown the guest as a protection step. They added NVMe swap as a
cushion. Console evidence 2026-08-18: Lean/Lake OOM killed systemd and
took inbound networking with it. Do **not** publish plan SKUs,
RAM/core/disk counts, swap size, or provisioned IPs. Agents never
reboot. Do not open another ticket to re-argue the Stopped badge.

Do not invent unlocked tracks while implementing this (no MX flip, no
DMARC `p=reject`, no live Vaultwarden / ban, no Q-AUTH-1 redesign, no
reboot unless asked). Law dual-pin: [AGENTS.md](../AGENTS.md) section
**Mention is in scope; remote builder and niceness**.

### Production networking assumption (operator 2026-08-02 + P1)

On the production NixOS box, **TCP port 80 is free** so the product
**redirect-only** listener can bind (`redirectHttpToHttps` +
`httpRedirectListen`, default `0.0.0.0:80`). That port serves HTTP->HTTPS
redirect/upgrade only. No cleartext management API on :80.

**P1 product edge:** Axum management-ui owns public **:80** and **:443**.
Stalwart is backend mail; it must **not** permanently own product clearnet
HTTPS on :443. Free engine first-boot HTTPS before public B1 (apply plan
below).

| Assumption | Status |
|------------|--------|
| Free :80 for product redirect-only bind | **Operator-approved** for production day-one |
| ACME HTTP-01 on product :80 | **Still parked** (Q-EDGE). Free :80 does **not** invent ACME-on-product-:80. Prefer external PEMs, in-process DNS-01 ACME (default off), or dual-run ACME |
| Free :443 for product rustls HTTPS (Axum) | **Required** for public cutover; place host PEMs **or** enable in-process ACME (staging first). Free Stalwart engine :443 first if first-boot bound it |
| Stalwart not product clearnet HTTPS | **P1 offline pin** (2026-08-10). Plan: `/etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson` |
| In-process ACME (management-ui) | **Scaffold, default off** (2026-08-10). `instant-acme` + DNS-01. Not live host cutover. See enablement sketch below |

### Free Stalwart public :443 for Axum (P1 day-2; V1 driver shipped)

After Stalwart first boot and before enabling public `managementUi`
`listenMode=https` on :443. Prefer the **V1 driver** (reads Domain B
`stalwart-token`; never logs values). Manual `stalwart-cli` remains valid.

**Callout: generated `stalwart-token` is not an accepted admin credential**

**O1 (operator residual, 2026-08-11):** free-443 dry-run still **HTTP 401** on
Domain B token until Domain A holds a **Stalwart-accepted** admin credential
and is re-installed; file presence alone is not enough. Same gate blocks
`DkimSignature` registration. After update: say **token updated** (never paste
the string), then free-443 dry-run then live. Residual: [RESIDUAL.md](../RESIDUAL.md)
B1/B6.

| Fact | Meaning |
|------|---------|
| Random generate into Domain A staging | Creates a file only. **`--generate-material`** / `secrets-prompt --generate` for this kind is **not** Stalwart engine registration. The engine does not learn that value by magic. |
| What free-443 needs | A credential **Stalwart already accepts** as admin/API (first-boot admin password, or an API token the engine already knows). Domain B file path is how the driver *reads* it; presence of the file alone is not enough. |
| Live HTTP **401** | Wrong or unknown admin credential. The token/password in Domain B is not what Stalwart has. Fix the **value**, not the free-443 script. |
| How to put a real value | When password is **unknown**: **`just fix-public-dashboard -- --target root@HOST`** (urandom recovery pin + mint + free-443 dry-run). Prefer **`just bootstrap-stalwart-token`** when you have the admin password (CLI mints an engine-accepted API key; WebUI optional). Or **`just add-stalwart-token`** when you already hold the key string. Do **not** invent a new random token and expect free-443 to work. |
| Optional Vaultwarden human copy | Keep the same real admin value in Domain C for recovery. That does **not** free :443. Path: VW copy -> export to Domain A (`secrets-export-bw-to-staging` or re-paste via secrets-prompt) -> install Domain B -> free-443. **No boot pull from VW.** See *Vaultwarden human path* one-pager above. |

**Domain B token first:** install kind `stalwart-token` to durable
`/var/lib/surmount/secrets/ui/stalwart-api-token` (S8; `/run/surmount-secrets/...`
still allowlisted as ephemeral) or set `STALWART_TOKEN` /
`STALWART_TOKEN_FILE`. Installing that file is **not** engine registration;
first-boot admin may still be required once so the engine accepts the API key.
See [SECRETS.md](SECRETS.md) (bootstrap honesty + five-gate table).

```bash
# Driver (default dry-run; exit 2 BLOCKED when no token ... not a green free claim)
just free-stalwart-public-443 -- --dry-run
# Live on operator host only (never CI green):
just free-stalwart-public-443 -- --live --restart
# Cutover compose optional step:
just host-cutover -- --dry-run --free-443 --staging ... --host-id ...
# Hermetic self-test (fake CLI; GHA quality): crate surmount-stalwart-ops
```

### Mail-plane TLS (IMAP :993 / SMTPS :465)

Stalwart first-boot still serves an engine self-signed leaf (Evolution
shows `rcgen self signed cert`). Nix cannot pin those files. Point the
engine at the **same durable Let's Encrypt PEMs** Axum uses:

```bash
# Default dry-run. Token is Domain B stalwart-token (same gate as free-443).
just point-stalwart-mail-tls
# Live on operator host only (never CI green):
just point-stalwart-mail-tls -- --live --restart
# Hermetic self-test (fake CLI; GHA quality): crate surmount-stalwart-ops
```

The leaf must name `mail.<apex>` on the certificate hostname list. Add
that name to the **private** laptop host profile `acme_domains`, then:

```bash
just laptop-renew-cert -- --check --directory production --host-profile PATH
just laptop-renew-cert -- --live --directory production --host-profile PATH --target root@YOUR_HOST
```

Host in-process ACME stays **off**. Namecheap `ClientIp` is **laptop
egress**. **Deploy this tree first** so group `surmount-tls` exists, then
install PEMs (cert `0640` `surmount-ui:surmount-tls`; key `0600`
`surmount-ui`). Switch copies mail-plane PEMs under `secrets/mail/tls`
(key `0600` `stalwart-mail`) and heals modes via tmpfiles `z`. The
apply driver sets `defaultCertificateId` to the **File** / Let's Encrypt
certificate (not the first-boot rcgen object). Stalwart **0.16.15**
`query Certificate --json` returns certificate hostnames plus `id` (not
`filePath`). The driver resolves the production leaf that way. Proof
(names only):

```bash
nix run .#e2e-host   # needs SURMOUNT_E2E_HOST=1 and BASE_URL
```

Issuer must be Let's Encrypt production, not `rcgen self signed cert`.
Do not treat Evolution "Accept Permanently" as the fix.

Manual equivalent (host token only; never in git):

1. Tunnel or use loopback to Stalwart HTTP management (often :8080).
2. `stalwart-cli query NetworkListener --fields id,name,protocol,bind --json`
3. Dry-run then apply:

```bash
export STALWART_URL=http://127.0.0.1:8080
# Token: Domain B file or STALWART_TOKEN (never paste into git/docs)
stalwart-cli apply --file /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson --dry-run
stalwart-cli apply --file /etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson
```

4. If first-boot listener **names** differ from the template (`name=https`),
   copy the plan host-local, adjust filters, re-apply. WebUI Settings >
   Network > Listeners is equivalent.
5. Restart `stalwart-mail` if binds persist until process restart (`--restart`
   on the driver).
6. Confirm :443 is free for Axum (`ss -lntp`), then enable UI https + PEMs
   or ACME.

Longer notes: [nix/stalwart/README.md](../nix/stalwart/README.md),
[EDGE_AND_TLS.md](EDGE_AND_TLS.md) P1 section,
[SECRETS.md](SECRETS.md) five-gate table.

### Private host profile -> ACME host-local (V2; non-secrets)

Edge hostnames and LE contact email are **public config**, not Domain A
secrets and not Vaultwarden activation material. Keep them in a private
**host profile** outside public git, then render into private host-local:

```bash
# Suggested profile path on the operator machine (never commit real values)
# ~/.local/share/surmount/host-profile.toml
# Public sample shape: script/testdata/host-profile/sample-host-profile.toml

just render-host-profile-acme -- \
  --profile ~/.local/share/surmount/host-profile.toml \
  --out /path/to/private/host-local
# -> host-local-acme.nix (staging directory default; external-hook paths)
# Multi-name (H5): sample acme_domains = services + mail + apex + www
#   (Nix list is space-separated). Live leaf also has mta-sts (already on
#   production cert 2026-08-12; do not re-issue just for that name).
# Production LE (H6): never silent. Prefer:
#   just render-host-profile-acme -- --profile PATH --out DIR --directory production --force
# Or set profile acme_directory to the production URL explicitly, then re-render.
# Re-render of a staging profile without --directory stays staging.

# Hermetic render tests: crate surmount-acme-namecheap in checks.*.ci
```

| What | Where |
|------|--------|
| `acme_domains`, `acme_email`, `acme_directory` | Host profile -> `host-local-acme.nix` (certificate hostname list; staging default; production only with explicit flag or profile field) |
| Namecheap ApiKey / ClientIp | Domain A (laptop intake). For laptop DNS-01 (the live path), `ClientIp` is **laptop egress**, not the VPS. Domain B `namecheap.env` is only needed if host in-process ACME is ever enabled (off today). Mode **0600** required. |
| Session / Stalwart / VW tokens | Domain A -> Domain B (install bridge) |
| Human recovery copies | Domain C Vaultwarden after S7b (never boot SoT) |

Detail: [SECRETS.md](SECRETS.md) section 1.0.2, [deploy-host-local.md](deploy-host-local.md).

### Optional Namecheap A/AAAA/TXT zone tool (V3)

Day-1 registrar UI for A/AAAA/TXT remains valid. Optional automation when
Domain B (or laptop staging) has the same credentials file as the ACME DNS-01
hook:

```bash
just dns-zone-namecheap -- list
just dns-zone-namecheap -- set-a services 203.0.113.10          # dry-run default
just dns-zone-namecheap -- --live set-host mail --a 203.0.113.10 --aaaa 2001:db8::10
# Send auth TXT (SPF / DMARC / DKIM); dry-run default
just dns-zone-namecheap -- set-txt @ 'v=spf1 a:mail.example.test -all'
just dns-zone-namecheap -- set-txt _dmarc 'v=DMARC1; p=none; rua=mailto:admin@example.test; pct=100'
just dns-zone-namecheap -- set-txt 'stalwart._domainkey' 'v=DKIM1; k=ed25519; p=...'
just dns-zone-namecheap -- set-txt 'stalwart-rsa._domainkey' 'v=DKIM1; k=rsa; p=...'
# Hermetic mock zone: crate surmount-dns-zone in checks.*.ci
```

**Do not** run `--live` concurrent with ACME DNS-01 challenge set/clear
(both use full-zone `setHosts`). PTR / rDNS stays at the VPS provider (not
this tool; optional SHC path below). The zone tool cannot toggle DNSSEC
(Namecheap API has no DNSSEC method). Click path, leftover DS, and SHA-1
digest type 1: [DNS.md](DNS.md) *Best DNSSEC we can actually run*.
Wave 1 does not add a DNS parser. A later DNS-zone package must **fail
closed** on DS digest type 1 (SHA-1). Do not treat leftover SHA-1 parent
records as acceptable.
SPF/DKIM/DMARC earn-trust checklist and
Stalwart DKIM register steps: [DNS.md](DNS.md). Domain B DKIM dual-sign
convention (Day-1):

| Selector | Algorithm | Domain B path |
|----------|-----------|---------------|
| `stalwart` | Ed25519 | `/var/lib/surmount/secrets/mail/dkim/stalwart-ed25519.pem` |
| `stalwart-rsa` | RSA-4096 | `/var/lib/surmount/secrets/mail/dkim/stalwart-rsa4096.pem` |

Engine registration of **both** `DkimSignature` objects: host driver
`just register-dkim` (default dry-run; `--live` to create). Token from
Domain B file, loopback `:8080` only, never logged. Preflight checks PEMs
and `ProtectSystem` ReadOnlyPaths. Idempotent if selectors already exist.
Hermetic: crate `surmount-stalwart-ops`. Green live is both objects
`stage=active` (**sign-ready**, not outbound signed mail). RSA floor 4096
(not 2048); hardness/quantum honesty: [DNS.md](DNS.md) *Hardness and quantum
honesty*.

### Optional SHC rDNS (PTR) tool

Namecheap is **forward zone only**. Reverse DNS for the sending IP is at the
**VPS provider**. On **Sovereign Hybrid Compute (SHC)**, prefer the customer
user-api when you have an operate-scoped key:

```bash
# Domain A intake (no-echo; never invent the key)
just secrets-prompt -- shc-api --host surmount-1
# optional Domain B install:
just secrets-install-host -- --from-staging "$HOME/.local/share/surmount/staging" \
  --host-id surmount-1 --target root@YOUR_HOST --require-kind shc-api

just rdns-shc -- --list
just rdns-shc -- --list-vms
just rdns-shc -- --hostname mail.surmount.systems --ip YOUR_VPS_IP   # dry-run
just rdns-shc -- --live --hostname mail.surmount.systems --ip YOUR_VPS_IP
just rdns-shc -- --list-departments
just rdns-shc -- --open-ticket --department-id 1 --subject "..." --message-file PATH
just rdns-shc -- --live --open-ticket --department-id 1 --subject "..." --message-file PATH
nix run .#surmount-shc -- --list
```

The same `just rdns-shc` client opens a customer support ticket
(`POST /user-api/v2/support/tickets`) with `department_id`, `subject`, and
`message`. Public department **SHC Team** uses URL id `1`. This is the
customer User API, not the Blesta staff plugin `tickets/add`. Default is
dry-run. `--live` uses the same confirmation dance as rDNS writes. Never
put provisioned IPs or ApiKey values in the tree, residual, or reports.

| Item | Detail |
|------|--------|
| Kind | `shc-api` (body: `ApiKey`, `ApiBase`, optional `ServiceId`) |
| Domain B path | `/var/lib/surmount/secrets/rdns/shc.env` |
| Credentials override | `SURMOUNT_RDNS_SHC_ENV` or Domain A staging `shc-api/secret` |
| Default hostname | `mail.surmount.systems` |
| Confirm flow | `--live` re-sends on 409 `confirmation_required` with `X-User-Api-Confirm` |
| FCrDNS | Forward A must already match the IP (API 422 otherwise) |
| Tickets | `--list-departments` and `--open-ticket` (customer User API) |
| Console | Provider services management console still valid |
| Citations | [Customer API KB](https://blesta.sovereignhybridcompute.com/plugin/support_manager/knowledgebase/view/16/the-customer-api-and-mcp/) (accessed: 2026-08-17); [OpenAPI](https://blesta.sovereignhybridcompute.com/user-api/openapi.json) (accessed: 2026-08-17); [Open Ticket](https://blesta.sovereignhybridcompute.com/client/plugin/support_manager/client_tickets/add/1/) (accessed: 2026-08-17) |

Never log ApiKey. Never commit production IPs. Detail: [DNS.md](DNS.md),
[SECRETS.md](SECRETS.md).

### In-process ACME enablement sketch (default off; staging first)

Tree ships optional in-process ACME for the management-ui Axum edge. **Default
is off.** Static host PEMs remain a first-class lean/lab path. **ACME live path
does not require operator-supplied CA PEMs.** Domain B only needs writable
parent directories (and later account JSON written by the binary) plus an
operator DNS-01 hook. Never put account credentials or PEMs in git. CI never
requires live Let's Encrypt. Prefer rendering `host-local-acme.nix` from a
private host profile (section above) instead of hand-pasting domains into
public tree samples.

1. **Ensure Domain B parents** (no CA PEMs to paste). **Durable default
   (H-PEM; survives reboot):**
   - `/var/lib/surmount/secrets/tls/` (writable by `surmount-ui` for issued
     `cert.pem` / `key.pem`; cert 0640 `surmount-ui:surmount-tls`, key
     0600 `surmount-ui` owner-only)
   - `/var/lib/surmount/secrets/acme/` (parent of `accountCredentialsPath`)
   - product state prefix `/var/lib/surmount` mode **0755** so `surmount-ui`
     can traverse to Domain B leaves (H2b)
   Ephemeral override still valid: `/run/surmount-secrets/{tls,acme}/` via
   `--tls-dir` / `--acme-dir` or profile path overrides (tmpfs; re-issue after
   reboot). Hermetic / pre-switch ensure (no secret payloads):

   ```bash
   nix run .#secrets-install-host -- --ensure-acme-parents --target root@YOUR_HOST
   # or hermetic: --dest-root /tmp/fake-root
   # host-cutover --acme-path also invokes ensure when --target/--dest-root set
   # ephemeral: --tls-dir /run/surmount-secrets/tls --acme-dir /run/surmount-secrets/acme
   ```

   Durable on boot: management-ui **tmpfiles** create durable `tls/` + `acme/`
   leaves and, when `acme.enable`, parents of configured PEM/account paths
   under durable and/or ephemeral material roots (`0755` root, `0750
   surmount-ui:surmount-ui` leaves). Unit gets **ReadWritePaths** on those
   parents and skips PEM `ConditionPathExists` so first start can issue.
   Does **not** invent PEM or account JSON contents.
2. **Start with Let's Encrypt staging** directory URL:
   `https://acme-staging-v02.api.letsencrypt.org/directory`
   (production only after staging succeeds).
3. **Nix (product path, web.enable false):**

```nix
surmount.managementUi = {
  listenMode = "https";
  tlsCertPath = "/var/lib/surmount/secrets/tls/cert.pem"; # host only; durable H-PEM
  tlsKeyPath = "/var/lib/surmount/secrets/tls/key.pem";
  acme = {
    enable = true;
    directory = "https://acme-staging-v02.api.letsencrypt.org/directory";
    email = "ops@example.test"; # operator contact; not invent production in git
    domains = [ "services.example.test" ];
    accountCredentialsPath = "/var/lib/surmount/secrets/acme/account.json";
    challenge = "dns-01";
    # dnsProvider = "none"; # default: reuse PEMs or fail closed without adapter
    # dnsProvider = "mock"; # lab only (self-signed; not live LE; not production LE dir)
    # dnsProvider = "external-hook"; # operator DNS-01 hook (see below)
    # dnsHookPath = "/run/surmount/acme-dns-hook"; # absolute; never a secret in git
    # renewDaysBeforeExpiry = 30; # scaffold default; early-renew window
  };
};
```

4. **DNS-01 challenge adapter (not a DNS brand):** `dnsProvider` /
   `SURMOUNT_ACME_DNS_PROVIDER` is the ACME DNS-01 *challenge adapter* (publish
   `_acme-challenge` TXT). It is **not** "Surmount DNS product" and **not** a
   locked commercial vendor (Cloudflare, Route53, etc.). Surmount is the
   host/stack provider; **you** control product-domain DNS (self-hosted or
   whatever you already run).
   - `none` (default): reuse valid PEMs or fail closed if missing / in renew
     window / domain-mismatch.
   - `mock`: lab self-signed PEMs offline (not WebPKI). Refused if directory is
     production Let's Encrypt.
   - `external-hook` (alias `hook`): run an **operator-owned** absolute
     executable (`dnsHookPath` / `SURMOUNT_ACME_DNS_HOOK`). Timeout
     `dnsHookTimeoutSecs` default 60, allowed range **1..600**. No shell; argv
     only.

5. **external-hook protocol** (hermetic tests use a temp script):

   | argv | Meaning | Success |
   |------|---------|---------|
   | `set <fqdn> <txt_value>` | Publish TXT | exit 0 |
   | `clear <fqdn>` | Remove TXT | exit 0 |
   | `wait <fqdn> <txt_value>` | Optional propagation check | exit 0 ready; **exit 2** = unsupported (proceed; you must block in `set` until visible); other non-zero = brief product retries only, then error |

   **Hook path rules (fail-closed):** absolute path; **regular file only**
   (symlinks refused; prefer a realpath under `/nix/store` or a host-owned
   `/run/...` file you control); must be executable; **not** group/world-writable.
   Product re-checks these bits before each spawn. Prefer the hook **outside**
   ACME PEM `ReadWritePaths` parent dirs so a writeable cert tree cannot replace
   the hook.

   **Env isolation:** the UI process **clears** the child environment and sets
   only a fixed minimal `PATH`. Session secrets, Stalwart tokens, and other
   parent env must not appear in the hook. If the hook needs credentials, read
   them from a host file the unit grants via `ReadOnlyPaths`, not from UI env.

   **Logging:** product logs hook **exit code + verb/fqdn** only. stdout/stderr
   are discarded. **Do not print credentials** from the hook (there is no product
   capture path for diagnostics).

   **Wait policy:** product-side `wait` retries are **brief** (a few hundred ms
   total). The product does **not** multi-minute poll public DNS. Long
   propagation belongs inside your hook: block in `set` until visible, or
   implement a real `wait` that blocks within the hook timeout.

   **Ambient capabilities residual:** when the unit has
   `CAP_NET_BIND_SERVICE` for low-port HTTPS/redirect, that ambient capability
   may still be available to the hook child. Hooks should not need low-port
   bind; dropping ambient caps in the child is residual hardening.

   **In-tree lab sample (not production):**
   `nix run .#acme-dns-hook-lab` implements set/clear/wait against a temp
   zone file (`SURMOUNT_ACME_DNS_LAB_ZONE`). Hermetic coverage is crate
   `surmount-acme-namecheap`. Production hooks must talk to **operator-owned**
   DNS (nsupdate, your API, self-hosted NS); never treat the lab sample as
   product law.

   **In-tree Namecheap DNS-01 hook (install-ready; not a product brand lock):**
   `nix run .#acme-dns-hook-namecheap-bin` implements the same set/clear/wait
   protocol against Namecheap `domains.dns.getHosts` / `setHosts`. Credentials
   live only on the host (never in git). Hermetic self-test (mock zone; no
   network): crate `surmount-acme-namecheap`. Day-1 registrar managed DNS
   notes: [DNS.md](DNS.md).

   **Install (operator host; no real IPs or keys in docs):**

   1. Place a regular-file hook (not a symlink) where the unit can execute it,
      mode not group/world-writable, e.g. copy or install the script to
      `/run/surmount/acme-dns-hook` (or a path under `/nix/store` after package).
   2. Write credentials at the default path the hook reads after product
      `env_clear` (UI session secrets never appear in the child). Prefer durable
      Domain B (survives reboot); `/run` remains optional ephemeral:

      ```text
      /var/lib/surmount/secrets/acme/namecheap.env
      # optional ephemeral: /run/surmount-secrets/acme/namecheap.env
      ```

      Example shape only (synthetic values; never commit real keys):

      ```text
      ApiUser=YOUR_NAMECHEAP_API_USER
      ApiKey=YOUR_NAMECHEAP_API_KEY
      UserName=YOUR_NAMECHEAP_API_USER
      ClientIp=YOUR_WHITELISTED_PUBLIC_IP
      SLD=example
      TLD=com
      SettleSeconds=30
      TxtTtl=60
      ```

      Mode **0600** required (hook and zone tool refuse group/world bits).

      **`ClientIp` is not "where you typed the secret."** It is the public IP
      **Namecheap sees for API HTTP calls**. Whitelist the API caller IP(s)
      in Namecheap API access (no multi-IP UI claims beyond that).

      | Caller | Typical machine | `ClientIp` / whitelist |
      |--------|-----------------|------------------------|
      | ACME DNS-01 hook (management-ui issues certs) | **Mail host / VPS** | VPS public IP. Domain B env for live ACME should use this. |
      | `dns-zone-namecheap` or laptop dry exploration | **Operator laptop** | Laptop public (egress) IP |
      | Domain A intake (`just secrets-prompt`) | **Operator laptop** (preferred) | Intake only; does not itself set Namecheap whitelist. Put the **future caller** IP into the prompted `ClientIp` field. |

      Prefer running secrets-prompt on the laptop (no-echo paste). When one
      `namecheap.env` is installed to Domain B for ACME, set **`ClientIp` to
      the VPS**. Whitelist **both** laptop and VPS when practical so zone
      tools from the laptop still work. Full dual-pin:
      [SECRETS.md](SECRETS.md) *Laptop intake vs Namecheap ClientIp*.

      `SettleSeconds` is optional block-after-set for propagation (product
      wait retries are brief; long waits belong in the hook).
   3. Point management-ui ACME at the hook:

      ```nix
      surmount.managementUi.acme = {
        enable = true;
        # prefer staging directory until proven
        dnsProvider = "external-hook";
        dnsHookPath = "/run/surmount/acme-dns-hook";
        # domains, email, accountCredentialsPath, ...
      };
      ```

   4. Unit must grant **read** on the credentials file and **execute** on the
      hook (e.g. `ReadOnlyPaths` for secrets; hook outside ACME PEM write
      trees). Prefer staging CA directory first.
   5. Lab training without Namecheap: use `acme-dns-hook-lab.sh` + temp zone.
      Namecheap hook self-test never calls the live API.

   Sketch (lab only; example domains; **no real secrets**):

```sh
#!/bin/sh
# /run/surmount/acme-dns-hook  (operator-owned; not product law)
# Talk to *your* DNS: nsupdate, your API, etc. Staging CA directory first.
# Regular file (not a symlink); mode e.g. 0755 (not group/world-writable).
# Or train against nix run .#acme-dns-hook-lab with a temp zone file first.
set -eu
cmd=$1
case "$cmd" in
  set)
    fqdn=$2; value=$3
    # nsupdate or API: create TXT at $fqdn with $value
    # Block here until publicly visible if you do not implement wait
    # (product does not multi-minute poll DNS for you).
    exit 0
    ;;
  clear)
    fqdn=$2
    # delete TXT at $fqdn
    exit 0
    ;;
  wait)
    # optional: poll until $3 is visible at $2 within hook timeout;
    # or exit 2 if unsupported (then set must have blocked already)
    exit 2
    ;;
  *)
    exit 1
    ;;
esac
```

6. **Reuse / early-renew rule:** PEMs are reused when leaf `notAfter` is still
   in the future, not not-yet-valid, certificate hostnames cover configured domains, **and**
   remaining lifetime is at least `renewDaysBeforeExpiry` /
   `SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY` days (scaffold default **30**).
   Process restart is required to pick up reissued PEMs (hot-reload residual).
7. **Restart after renew** (hot-reload residual). Key mode must stay owner-only
   (e.g. 0600); binary refuse group/world readable keys. Account credentials
   JSON same owner-only contract. Directory URL must be `https://`. Prefer
   **staging** directory until a host cutover is proven.
8. **Wildcards** (`*.example`) are refused until DNS-01 TXT naming is designed.
9. **HTTP-01 on product :80** remains parked. Do not invent ACME on the
   redirect-only listener.

Firewall still opens only required ports (`modules/networking.nix`). Dual-run
escape (`surmount.web.enable = true`) owns :80 ACME/redirect while nginx is
active; product redirect-only and dual-run nginx must not both claim :80
(eval mutex). Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md).

### Bring-up order

**Private material stays on the host.** Paste SSH public keys, real public
IPv4/IPv6, TLS PEMs, and age identities into **host** paths or the live
machine config only. Do **not** commit them to this public tree. The
pre-commit private-data scan (`surmount-private-data`) blocks common
accident classes; it is not a license to invent allowlists of real values.
See [hygiene.md](hygiene.md) and [SECRETS.md](SECRETS.md).

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
4. **D1 hybrid TLS negotiation** (after B1 HTTPS is live): same BASE_URL,
   `just check-tls-hybrid` / `nix run .#surmount-tls-hybrid`. Exit **2**
   BLOCKED when BASE_URL unset (no false-green). Runbook:
   [deploy-host-local.md](deploy-host-local.md) section 6. Offline provider
   proof stays `cargo test ... tls::` (not host negotiation).
5. **Arti HS keys** under a **durable** `onionServiceStateDir` (owned by
   `surmount-arti`; prefer Domain B, not the `/run` default). Start daemon
   when ready; unit active alone is not onion published. Live host
   (2026-08-17): unit `surmount-arti-hidden-service` **active**; hostname
   file present; Onion-Location + Alt-Svc on mapped HTTPS. Restart
   `surmount-management-ui` after hostname/env/map change (no hot-reload).
   Operator offline HS backup and Tor Browser verify still residual
   ([RESIDUAL.md](../RESIDUAL.md)).
6. **Optional ban lab** on host sets (`surmount-ban4` / `surmount-ban6`) via
   e2e-host; cleanup with helper `remove-ban`.
7. **DNS and mail** cutover when edge is healthy: [DNS.md](DNS.md), mail ports.
   Set a mailbox password from the services console **Mail** page (operator is
   the admin). Create principals still via `stalwart-cli` / bootstrap admin
   when the Accounts list is offline.

Host module entry: [hosts/mail-vps/configuration.nix](../hosts/mail-vps/configuration.nix) (flake attr `#mail-vps`; alias `#surmount-mail`).
Open residual tracks: [RESIDUAL.md](../RESIDUAL.md).

## Intent

The stack should help the operator **troubleshoot itself**:

- Useful logs in journald without needing a SaaS sink as a hard dependency
- Health endpoints for the pieces we own
- Small scripts / flake apps for DNS, TLS, and mail checks
- Abuse controls we run (fail2ban, Stalwart, edge limits) without Cloudflare WAF
- Runbooks in `docs/` that match reality

Prefer boring, local tools. Operator bins are flake apps (`nix run .#...`).
Keep them pure and secret-free.

## Logging

Paper trail is **journald** (operator-only). `surmount.logging.enable` defaults
true with `surmount.enable`. Persistent journal under `/var/log/journal` with
a **size cap** so logs cannot fill the disk (SHC ticket 261 class). Scaffold
`SystemMaxUse=1G` and `RuntimeMaxUse=256M` are not published guest disk or
RAM. `ForwardToSyslog=no`. Mail, management-ui, sshd, Arti, and networking
stay at normal priority (not niced). No public log dump on the apex site.

| Source | Where | Notes |
|--------|-------|-------|
| journald | `/var/log/journal` (persistent, vacuumed) | `modules/logging.nix`; size + 30day retention; explicit RateLimitIntervalSec/Burst so floods cannot use an unpinned default |
| Stalwart | journald via stdout (`RUST_LOG=info`) | SyslogIdentifier `stalwart-mail`; RocksDB store errors on this unit, not a second log file; no recovery passwords; no mail bodies at info |
| management-ui | journald (systemd service) | Structured `http_request` (method/path/status/latency) + `tls_handshake_failed`; WARN+ on stderr so `journalctl -p err` sees failures |
| sshd | journald | `LogLevel VERBOSE` (auth fail / disconnect; no passwords) |
| fail2ban | journald | Transitional sshd jail; `backend = systemd` |
| nft `surmount_guard` | kernel -> journald | `log prefix "surmount-nft-ban-drop: "` when logging on |
| Arti | journald via stdout | `console = info`; `log_sensitive_information = false` (no HS keys) |
| nix-daemon | journald | Remote builder path; niced + MemoryMax; per-unit log burst kept high so OOM start is visible; not mail |
| surmount-lake (if enabled) | journald | Default-off; niced + MemoryMax on the lake unit; SyslogIdentifier `surmount-lake` |
| Vaultwarden (if enabled) | journald via stdout | `LOG_LEVEL=info`; never `LOG_FILE`; never ADMIN_TOKEN |
| nginx | journald | Transitional-to-delete only |

### Retention

- Size vacuum: `SystemMaxUse` / `RuntimeMaxUse` in `surmount.logging`.
- Age vacuum: `MaxRetentionSec` scaffold `30day`.
- Burst cap: `RateLimitIntervalSec` scaffold `30s` and `RateLimitBurst` scaffold `50000` (not 0; 0 disables the rate limit; must stay at least 20000). Completeness and the start of an OOM or cgroup kill beat aggressive vacuum. Dropped lines show as journald "Suppressed N messages". nix-daemon and `surmount-lake` also set a per-unit `LogRateLimitBurst` of 50000.
- Do not log secrets (passwords, tokens, PEMs, Authorization, Cookie, recovery).
- Mail content does not belong in application info logs.
- Do not write product access logs under `/var/log` or `/var/lib/surmount`.

### Laptop-home justfile (second window, no guest-only scripts)

Laptop: copy [contrib/home-justfile](../contrib/home-justfile) to
`~/justfile` (wrapper into this clone). Guest: after `just deploy-host`,
`/root/justfile` is a symlink to `/etc/surmount/root-justfile` (same
recipes as [contrib/guest-root-justfile](../contrib/guest-root-justfile):
`just status`, `just logs`, `just btop`, `just scram`). Do **not** hand-place a justfile
only on one host. Laptop `just btop` uses Eternal Terminal. Guest `just btop`
is local. Laptop `just scram` SSHes `surmount-scram --now` (default
`root@surmount-1`). Guest `just scram` is local. Watchdog:
`surmount-scram.service` (`--watch`, Nice=-20). Swap file is host-local
path only (`surmount.swapFile`).

Second Alacritty before `just deploy-host`: `just et` from home (or the
repo) and leave it logged in. Activation can drop the SSH that started
the switch. Recovery: `just status` (units + nixbuilder slice MemoryMax).

```bash
just et
just status
just logs
just deploy-host -- --target root@surmount-1 --host-local "$HOME/.local/share/surmount/host-local"
```

### Reading logs (operator)

`just host-logs` (`nix run .#surmount-host-logs`) follows journalctl on
the same SSH target as `just btop` / `just host-inxi` (crate; no sudo).
No TTY. `just host-logs -- --status` is the one-shot diagnose snapshot
(journal disk use, journald knobs, failed units, is-active, short tails).
Target: `--target`, `SURMOUNT_DEPLOY_TARGET`, or
`~/.local/share/surmount/agent-target.env`. Hermetic: crate
`surmount-host-logs`.

When `surmount.remoteBuilder.enable`, `nixbuilder` is in group
`systemd-journal` so it can read the system journal without sudo. Do not
add nixbuilder to wheel. Extra users: `surmount.logging.journalReaders`.

```bash
just host-logs
just host-logs -- --status
just host-logs -- --target nixbuilder@example.test
just host-logs -- -- -u sshd -n 20
journalctl -u stalwart-mail -e
journalctl -u surmount-management-ui -e
journalctl -p err -u surmount-management-ui -e
journalctl -u surmount-arti-hidden-service -e   # when startDaemon=true
journalctl -u surmount-arti-scaffold-status -e  # enable without daemon
journalctl -u fail2ban -e
journalctl -u nix-daemon -e
journalctl -k -g surmount-nft-ban-drop
journalctl -u nginx -e          # transitional-to-delete edge
```

Management-ui HTTPS request lines stay on stdout (info) and go to journald
for unit `surmount-management-ui`. WARN and ERROR go to stderr so journal
priority is err. There is no public HTTP log viewer, no log-tail route, and
no world-readable product file under `/var/log` or `/var/lib/surmount`. Read them over SSH as an operator (root, wheel, or
`nixbuilder` in `systemd-journal`). Failed TLS handshakes emit
`tls_handshake_failed` (peer, SNI when known, short error kind; no PEM).
Completed HTTP requests emit `http_request` with method, onion-redacted
path, query presence, status, latency, Host, User-Agent (truncated), and
peer IP as a journal field (not baked into git fixtures). Query strings,
cookies, and Authorization are not logged. To diagnose a Safari connect
failure, retry the phone while watching `journalctl -u
surmount-management-ui -g tls_handshake_failed`.

### Arti onion / hidden service (ops)

| Item | Notes |
|------|-------|
| Package | Prefer `pkgs.artiOnionService` (Surmount overlay; `onion-service-service`). Stock `pkgs.arti` is client-default. |
| Config | `/etc/surmount/arti.toml` (generated; no private keys) |
| HS identity dir | `surmount.artiHiddenService.onionServiceStateDir` default `/run/surmount-secrets/arti/onion-service` (**ephemeral**; prefer durable Domain B). Live host uses `/var/lib/surmount/secrets/arti/onion-service`. |
| Ownership | **Must** be owned/writable by `surmount-arti:surmount-arti` (e.g. mode **0750**). Module does **not** auto-create this dir (`ConditionPathIsDirectory` gates the daemon; missing dir => inactive, not a restart loop). Root-owned 0700 can pass the path check then fail at keystore open. Keystore stays **0700**. Live: `surmount-ui` is in group `surmount-arti` so the UI can read the hostname file (0750 dir). |
| Process cache | `/var/lib/surmount/arti` (+ `cache/`) via tmpfiles 0750 surmount-arti. Live host-local sets unit `HOME=/var/lib/surmount/arti` so Arti can write `port_info.json` (public module does not set HOME). |
| Hostname file | Arti 2.5.1 does **not** write `hostname`. Live host wrote it from `arti hss --nickname surmount-management onion-address`. Address is not a secret; HS keys are. Same v3 for apex, www, services. |
| Discovery headers | Onion-Location + Alt-Svc on mapped HTTPS 2xx/3xx. Optional env: `SURMOUNT_ONION_LOCATION_ENABLED`, `SURMOUNT_ONION_ALT_SVC_ENABLED`, `SURMOUNT_ONION_LOCATION_DISABLED_HOSTS`, `SURMOUNT_ONION_ALT_SVC_DISABLED_HOSTS`, `SURMOUNT_ONION_MAP_FILE`. Restart `surmount-management-ui` after hostname/env/map change. Dump: `GET /api/v1/system` `onion_discovery` (admin-gated when Nostr on). |
| Secrets | HS private keys **never in git**. Identity may be generated on first start in an empty writable dir; operator **offline backup** is still residual. |
| Honesty | `systemctl is-active surmount-arti-hidden-service` does **not** prove an onion is published on the Tor network. Live (2026-08-17): unit **active**; hostname file present (v3 onion; do not paste the address in this public tree); headers proven on HTTPS 307/200. Tor Browser purple pill **BLOCKED**. Do **not** claim B3 fully closed. |

Example host prep (operator; paths are examples only):

```bash
# Prefer durable Domain B (reboot-safe). The /run default is ephemeral.
install -d -m 0750 -o surmount-arti -g surmount-arti /var/lib/surmount/secrets/arti/onion-service
# Place HS identity material, or let a capable arti generate on first start
# in that empty writable dir. Copy the hostname from `arti hss onion-address`
# if Arti does not write the hostname file. Never commit key material.
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
  Vaultwarden module offline is **S7a done**. **S7b** (host enable + token +
  unit) is scripted by A0 `host-cutover --with-vaultwarden` after material
  exists. When VW is enabled on the host, include `/var/lib/vaultwarden` in
  restic paths (cutover fragment notes this) and install ADMIN_TOKEN via kind
  `vaultwarden-admin` to `/run/surmount-secrets/vaultwarden/admin.env` (never
  in git). Console shows Open vault when `SURMOUNT_VAULTWARDEN_URL` is set (or
  derived from loopback when VW enable).

**RocksDB consistency:** prefer `systemctl stop stalwart-mail` (or an atomic
volume snapshot with documented assumptions) before restic of `db/`. A live
file-level copy of an open RocksDB can restore torn state. We have not
load-tested backup windows under production mail volume.

Test restore before you need it (IMAP/JMAP readback of known messages).
Snapshot or restic **before** large Maildir imports ([MIGRATION.md](MIGRATION.md)).

## Abuse and perimeter (no CF WAF)

| Control | Role |
|---------|------|
| Firewall | Mail + edge + SSH + Eternal Terminal TCP 2022 (`networking.nix` plus et module) |
| fail2ban | SSH jail by default; expand only with clean filters (transitional) |
| Stalwart spam-filter + greylisting | Primary mail abuse path |
| Edge rate limits | HTTPS request floods ([EDGE_AND_TLS.md](EDGE_AND_TLS.md)) |
| Ban / whitelist (first path) | `surmount.accessControl` + management-ui ban layer; default off; see EDGE end-to-end |
| Kernel firewall `surmount_guard` sets | Opt-in empty sets (`surmount-ban4`/`surmount-ban6`); host load required for live drop |
| Axum auth -> ban signals | BanCandidate stub shipped; Q-ACL-1 open |
| SSH keys | Password auth off by default preference. Host keys: **ed25519 only** (`services.openssh.hostKeys` in `hardening.nix`). The daemon used to also offer RSA; clients already preferred ed25519. This module stops generating/advertising rsa. **Live leftover:** the running sshd still advertises RSA until a deploy. Existing RSA files on disk are not deleted by this change. Classical hygiene only: Ed25519 is not PQ; OpenSSH host keys are not PQ; PQConnect is not a host-key swap. |

## Eternal Terminal (reconnect after sleep and network change)

Operator SSH that comes back after laptop sleep or a cellular hop.
Does **not** replace Mullvad. Keep using Mullvad on the laptop and phone
as you already do.

See [Eternal Terminal](https://eternalterminal.dev/) (accessed: 2026-08-25).
Server is stock nixpkgs `services.eternal-terminal` (TCP **2022**), enabled
with hardening. Not niced. Logging is on (`silent = false`). `tmux` is on
the guest PATH so a long `nixos-rebuild` can outlive every disconnect.

After a NixOS switch that includes this module:

```bash
just et -- --target root@surmount-1
# or: nix run .#surmount-et -- --target root@surmount-1
```

For a rebuild that must finish even if every laptop tunnel dies, SSH or
`just et` in, then `tmux new -s switch` and run the command inside tmux.

## Operator bins inventory

Operator face is `nix run .#<app> -- args`. `just` aliases only `nix run`.
Product `script/*.sh` drivers are gone. Hermetic crate tests live in
`just ci` / `checks.*.ci`. Reasonable leftover exceptions (leftover-homes
fixtures, git hook shebang, laptop-renew systemd units):
[packages-and-forks.md](packages-and-forks.md) *Reasonable leftover
exceptions*.

| Command | Purpose |
|---------|---------|
| `nix run .#e2e` / `just e2e` | Local comprehensive end-to-end (Rust flake app; hermetic cargo matrix; optional Tor). SoT |
| `nix run .#e2e-host` / `just e2e-host` | Host end-to-end (Rust flake app; `SURMOUNT_E2E_HOST=1` or exit 2; also requires `SURMOUNT_E2E_BASE_URL`; `LAB_IP` unless `SKIP_BAN=1`). **Never** a flake check |
| `just deploy` | Publish static sites (apex/www from locked `github:SurmountSystems/site` via `nix build .#surmount-public-site`, plus extra vhosts). Not a NixOS generation. |
| `nix run .#surmount-deploy-host` / `just deploy-host` | Operator deploy driver: public rsync + host-local checks + `#mail-vps` rebuild (not CI). That is the NixOS generation. [deploy-host-local.md](deploy-host-local.md) |
| `nix run .#surmount-deploy-host-post-switch-smoke` | Post-switch smoke (generation, units, loopback health, listen proof). |
| `just fix-public-dashboard` / `nix run .#surmount-fix-public-dashboard` | Compose recovery pin + mint API key + free-443 dry-run (no operator password homework). Optional `--live-free-443` on operator host only. [SECRETS.md](SECRETS.md) |
| `just stalwart-recovery-unlock` / `nix run .#stalwart-recovery-unlock` | Generate/install `STALWART_RECOVERY_ADMIN` EnvironmentFile + private systemd drop-in + Basic auth probe. Kind `stalwart-recovery-admin`. [SECRETS.md](SECRETS.md) |
| `just bootstrap-stalwart-token` / `nix run .#bootstrap-stalwart-api-token` | Mint Stalwart API key from admin password (CLI Basic auth + `create apikey`); stage Domain A; optional Domain B install + free-443 dry-run. [SECRETS.md](SECRETS.md) |
| `just add-stalwart-token` / `nix run .#add-stalwart-token` | Stalwart admin token intake when you already hold the key (pipe / `--token-file` / paste). [SECRETS.md](SECRETS.md) |
| `nix run .#surmount-secrets-prompt` / `just secrets-prompt` | No-echo Domain A intake on the operator laptop. Prefer over chat/shell history. [SECRETS.md](SECRETS.md) |
| `nix run .#secrets-install-host` / `just secrets-install-host` | Domain A -> B secrets install (staging / optional secret-tool; kinds include `namecheap-api`). [SECRETS.md](SECRETS.md) |
| `nix run .#secrets-export-bw-to-staging` / `just secrets-export-bw-to-staging` | Optional Domain C (VW / `bw`) -> Domain A staging. **Never** activation or boot. [SECRETS.md](SECRETS.md) 5.2 |
| `nix run .#surmount-host-cutover` / `just host-cutover` | A0+V5+ladder. Default dry-run. |
| `just cutover-step <name>` | Short alias for one ladder step |
| `nix run .#surmount-host-material-inventory` | Material inventory (G1). |
| `pkgs.acme-dns-hook-namecheap` / `nix run .#acme-dns-hook-namecheap-bin` | Packaged DNS-01 helper **code** (store path for `dnsHookPath`; credentials stay Domain A / durable Domain B env) |
| `nix run .#surmount-render-host-profile-acme` / `just render-host-profile-acme` | Non-secret host profile -> `host-local-acme.nix` |
| `nix run .#free-stalwart-public-443` / `just free-stalwart-public-443` | Free Stalwart public :443 for Axum (default dry-run; Domain B token). |
| `nix run .#register-dkim` / `just register-dkim` | Dual-sign DkimSignature objects (default dry-run). |
| `nix run .#point-stalwart-mail-tls` / `just point-stalwart-mail-tls` | Point Stalwart IMAP/SMTP TLS at durable Axum Let's Encrypt PEMs (default dry-run). |
| `nix run .#surmount-laptop-renew-cert` / `just laptop-renew-cert` | Laptop Let's Encrypt renew (`--check` / `--live` / `--install-timer`). Host ACME stays off. Units: `script/laptop-renew/`. |
| `nix run .#surmount-private-data` / `just check-private-data` | Private-data pattern scan (`--staged` / `--tree` / `--paths`). |
| `nix run .#surmount-host-logs` / `just host-logs` | Host journal follow and `--status`. |
| `nix run .#surmount-et` / `just et` | Eternal Terminal client. Reconnects after sleep and network change. Does not replace Mullvad. After the host switch, `etserver` listens on TCP 2022. Long commands still belong in tmux on the guest. |
| `nix run .#surmount-rekey` / `just rekey` | SHC Backups PGP paste: mint age identity (age/rage crate), wrap with gpg, print `age1...` for type PGP. Identity stays 0600 on disk. |
| `nix run .#surmount-shc` / `just rdns-shc` | SHC customer user-api (rDNS PTR + tickets). No python3 in the crate. |
| `nix run .#surmount-tls-hybrid` / `just check-tls-hybrid` | **D1** host hybrid TLS negotiation probe after B1. Requires `SURMOUNT_E2E_BASE_URL=https://...` or exit **2** BLOCKED. Never a flake check. |
| `nix run .#surmount-domain-audit` / `just domain-audit` | Read-only DNS/web/TLS/mail posture (SPF/DKIM/DMARC, dual-sign selectors `stalwart`/`stalwart-rsa`, HTTPS, optional `--smtp` STARTTLS). |
| `nix run .#surmount-dns-zone` / `just dns-zone-namecheap` | Namecheap A/AAAA/TXT/MX/CAA merge. Default dry-run. |
| `nix run .#surmount-mail-import-maildir` | Operator Maildir++ import via Vandelay. |
| `vandelay` | Official 0.16 JMAP importer-exporter (`nix/packages/vandelay.nix`) |
| `stalwart-cli` | Upstream JMAP admin CLI (no Maildir import on 1.0.x) |
| `nix run .#surmount-diskstation-discover` / `just diskstation-discover` | Laptop mDNS discover of **DS1513** and **DS3018xs**. No IPv4 printed. No nmap. |
| `nix run .#surmount-diskstation-afp-mount` / `just diskstation-afp-mount` | Laptop AFP mount of MailPlus via gio. `--host DS1513` or `--host DS3018xs` required as Secret Service label. |
| `nix run .#surmount-copy-mailplus-uid` / `just mailplus-copy-uid` | Copy one numeric MailPlus uid to `/var/lib/surmount/import/maildir/<uid>/<uid>/Maildir`. |

End-to-end lives in `crates/surmount-e2e` and flake `apps.e2e` / `apps.e2e-host`.
Read-only checks unless documented otherwise. No secrets in env files
committed to git.

### Packaging

Each ops crate is one `callPackage` file under `nix/packages/` (not one
`ops.nix`). Do not wrap leftover bash in `writeShellApplication`. SHA-1
digest type 1 on DNS-zone tooling must fail closed.

## Operator runbook sketch

### First boot / DNS

Day-1 order and registrar vs VPS PTR: **[DNS.md Day-1 checklist](DNS.md#day-1-dns-checklist-operator)**.
Use registrar managed DNS (Namecheap BasicDNS or equivalent). Do not invent
self-host NS for Day-1.

1. **Edge-only:** A/AAAA for `services` (and apex/www if used) at the
   **registrar**. Then certs / public HTTPS ([EDGE_AND_TLS.md](EDGE_AND_TLS.md)).
   Product edge: apex and `www` serve the packaged **SurmountSystems/site**
   (UNDER CONSTRUCTION only if the document root has no index.html);
   operator console is only on `services.<domain>`. Extra proven static
   Hosts (DS3018xs HTML) use `managementUi.staticVhosts` and the same
   public mail-host A/AAAA. HTTP :80 upgrades same-host (apex users are
   not redirected into the console). Certificate hostnames in the host
   profile include services, mail, apex, www, and (when serving extra
   sites) each extra apex+www FQDN (`acme_domains`). `--live` renew does
   not detect a missing hostname. Staging LE first; production LE only
   with explicit `--issue --directory production` when adding names.
   Namecheap ClientIp is laptop egress. See [EDGE_AND_TLS.md](EDGE_AND_TLS.md).
2. **Mail host + inbound (when ready):** A/AAAA for `mail`, MX at registrar;
   PTR/rDNS at the **VPS provider** (SHC: `just rdns-shc` or console; not the
   registrar forward zone / Namecheap).
3. **When sending (not edge-only blocker):** SPF, DKIM, DMARC at registrar;
   full earn-trust list in [DNS.md](DNS.md). B6 in
   [deploy-host-local.md](deploy-host-local.md).
4. `just domain-audit` (presence plus mail-auth posture; swap names for your zone)
5. Deeper posture: `just domain-audit -- --no-color surmount.systems`.
   Optional `--smtp` if outbound TCP/25 is open.
   Unsigned DNSSEC (no DS) is INFO. Leftover unmatched DS, and DS digest
   type 1 (SHA-1), are FAIL. Missing MTA-STS may still WARN on purpose
   before earn-trust. See [DNS.md](DNS.md) *Best DNSSEC we can actually
   run*. Dual-sign selectors are in the default list.
   **Live (2026-08-12):** public HTTPS already covers `mta-sts.<domain>` and
   host-local `managementUi.mtaStsMode = "testing"` serves
   `/.well-known/mta-sts.txt` (default in sample tree remains **off** until
   private host-local enables it). Stay testing. Primary public MX is
   this host (2026-08-20). `list` prints EmailType. `--live set-mx`
   fails closed while EmailType is FWD (not a public MX flip; extra
   domains). Primary EmailType is **MX**.
6. After HTTPS PEMs or ACME path: `just e2e-host` with `SURMOUNT_E2E_BASE_URL=https://services.surmount.systems`

### Mail path

Living mailbox map (who is which account):
`~/.agents/surmount-server/operator-facts.md` on the operator machine.
IMAP username is that mailbox's own address. Mailbox password is set on
the services console `/mail`, not via SSH `update AccountPassword`.

1. `systemctl is-active stalwart-mail`
2. `just e2e-host` (mail ports when BASE_URL / host env is set)
3. Submit test message; read via IMAP client
4. Import: [MIGRATION.md](MIGRATION.md)

### UI path

**Policy (2026-08-12):** the **public** product edge (`listenMode=https` on
public :443, or any non-loopback bind that serves the operator console)
**requires** `authMode = "nostr"` plus host session secret and allowlist.
`authMode = "off"` is **loopback / lab only** (SSH tunnel, `just dev`). It is
**not** public-safe: middleware does not gate routes when mode is off.

| Bind | Allowed authMode | Notes |
|------|------------------|--------|
| Loopback cleartext (`127.0.0.1:8090` default) | `off` or `nostr` | Day-1 / lab; tunnel only |
| Public HTTPS services Host | **`nostr` required** | Session secret + allowlist on host; `publicBaseUrl` recommended |
| Apex / www Host | n/a (packaged SurmountSystems/site) | Not the operator console; still ship current binary |

**Day-1 / until public HTTPS (B1):** keep the management UI on loopback only
(`surmount.managementUi.listenAddress = "127.0.0.1"`, default port **8090**).
Never bind the cleartext UI to a public address with auth off. Reach it via
SSH tunnel until PEMs + `listenMode=https` own :443. Pin the loopback bind
in **private host-local** if the live box could drift. See
[EDGE_AND_TLS.md](EDGE_AND_TLS.md) and
[deploy-host-local.md](deploy-host-local.md) (B0 interim / B1 / **B4**).

1. `curl -fsS http://127.0.0.1:8090/health` (Surmount management UI default port)
2. `curl -fsS https://services.surmount.systems/health` when public HTTPS is live
   (Axum product edge on :443; not Stalwart). Health stays public even under
   mode=nostr.
3. After B4: anonymous console must **not** load (see *Production Nostr auth
   (B4)* proof curls below).
4. Stalwart HTTP management: SSH tunnel to **8080** (first-boot default; prefer
   loopback rebind; not product public HTTPS). Example:
   `ssh -L 8080:127.0.0.1:8080 mail-vps` then open `http://127.0.0.1:8080`
5. Before public B1: free Stalwart engine :443 if present (P1 apply plan above)

### Incident: disk full

1. `journalctl --disk-usage` / vacuum if appropriate
2. Check `/var/lib/stalwart-mail` growth; internal FTS and LSM compaction can
   retain space after deletes until cleanup/compaction run (DATASTORES.md)
3. Confirm Stalwart DataRetention / blob cleanup schedules if expunge is not
   reclaiming space
4. Do not delete `db/` cold without a verified backup and restore plan

### Incident: cert expiry

1. `security.acme` / edge renew logs (dual-run path) or host PEM renew path
2. `just e2e-host` / `just domain-audit` (TLS names and dates; never paste PEMs)
3. If using ACME HTTP-01 on the **dual-run nginx** path, confirm port 80 is
   reachable for challenges. Product :80 is **redirect-only** by default;
   ACME HTTP-01 on product :80 remains **parked** (Q-EDGE). Free production
   :80 is for redirect bind, not an ACME invent.

## Local Nostr auth enable (`just dev`)

Default local console is open: `SURMOUNT_AUTH_MODE=off` (see `justfile`
`dev`). Foundation is rust-nostr NIP-98 + HMAC session cookie. **Not** JS NDK.
nsec never goes on the server. Local `off` is fine on loopback; it is **not**
a public edge policy.

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
# export SURMOUNT_PUBLIC_BASE_URL=https://services.example.test  # NIP-98 u when Host differs
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
| `SURMOUNT_PUBLIC_BASE_URL` | Optional absolute origin for NIP-98 `u` tag (e.g. `https://services.surmount.systems`) |
| `SURMOUNT_NIP98_MAX_SKEW_SECS` | Optional skew window (default 300) |

Host Nix mirrors: `managementUi.authMode`, `nostrAllowlist`,
`nostrAllowlistFile`, `sessionSecretPath`, `publicBaseUrl` (prefer path over
inline env for production). Auth-failure ban matrix: [SECURITY.md](SECURITY.md).
Full product answers still open under **Q-AUTH-1** (key-loss, durable session
store, first-operator bootstrap UX). Depth: [SEARCH_AND_UI.md](SEARCH_AND_UI.md),
[SECURITY.md](SECURITY.md).

Local green with mode=nostr is **not** public cutover and does not close
Q-AUTH-1.

## Production Nostr auth on the public edge (B4)

**Goal:** anonymous clients cannot load the operator console on
`https://services.<domain>/`; allowlisted operators log in with Nostr
(NIP-07 or NIP-98). Apex/www stay the packaged SurmountSystems/site
(no operator-console lede). Missing document-root index.html still
falls back to yellow UNDER CONSTRUCTION.

**Do not invent** Q-AUTH-1 answers (durable server session store, key-loss
product UX, first-operator bootstrap). This section is **host enable** of the
shipped foundation.

### Prerequisites (Domain B + private host-local)

1. **Session HMAC secret** (`kind=session-secret`) as a systemd
   **EnvironmentFile** body: one line
   `SURMOUNT_SESSION_SECRET=<hex>` (never git). Durable path convention:
   `/var/lib/surmount/secrets/ui/session-secret` (mode **0600**).
2. **Nostr allowlist** of operator **npub** or hex pubkeys only (**never nsec**).
   Prefer a host file (`kind=nostr-allowlist`): durable
   `/var/lib/surmount/secrets/ui/nostr-allowlist` (mode **0600**). Empty
   allowlist + mode=nostr = **nobody can log in** (gate still blocks anonymous).
3. Private host-local sets `authMode = "nostr"`, wires paths, sets
   `publicBaseUrl` to the services origin. **No secrets or real npubs in the
   public sample** (`hosts/mail-vps`).

Install kinds and shapes: [SECRETS.md](SECRETS.md) (*session-secret* and
*nostr-allowlist* one-shot). Posture: [SECURITY.md](SECURITY.md),
[EDGE_AND_TLS.md](EDGE_AND_TLS.md).

### One-shot: material then host-local (placeholders only)

```bash
# --- Domain A: generate session secret (raw hex) then wrap for EnvironmentFile ---
just secrets-prompt -- session-secret --host YOUR_LOGICAL_HOST --generate
# Edit private staging secret file to EnvironmentFile shape before install:
#   SURMOUNT_SESSION_SECRET=<the-64-hex-chars>
# (systemd EnvironmentFile for sessionSecretPath requires KEY=value.)

# --- Domain A: allowlist file (npubs only; never nsec; never commit) ---
# Private staging item kind=nostr-allowlist, path:
#   /var/lib/surmount/secrets/ui/nostr-allowlist
# secret body example (placeholders):
#   npub1exampleplaceholder000000000000000000000000000000000000
#   # comments and blank lines OK; comma/space/newline separated also OK

just secrets-install-host -- --from-staging "$HOME/.local/share/surmount/staging" \
  --host-id YOUR_LOGICAL_HOST --target root@YOUR_HOST \
  --require-kind session-secret --require-kind nostr-allowlist
```

Private host-local fragment (outside public git; placeholders only):

```nix
# PRIVATE host-local only. Never commit real npubs or secret paths into public hosts/.
{
  surmount.managementUi = {
    authMode = "nostr";
    # EnvironmentFile body: SURMOUNT_SESSION_SECRET=...
    sessionSecretPath = "/var/lib/surmount/secrets/ui/session-secret";
    # Prefer file for rotation; env string also works (nostrAllowlist).
    nostrAllowlistFile = "/var/lib/surmount/secrets/ui/nostr-allowlist";
    # NIP-98 u-tag matching for services origin (recommended on public edge).
    publicBaseUrl = "https://services.example.test";
  };
}
```

Then rebuild/switch the host so management-ui restarts with mode=nostr. Keep
SSH access; if allowlist is wrong you can still fix the host file and restart
the unit.

### Live proof (after switch)

Swap `services.example.test` / apex for your zone. Expect **gated** console on
services; **not** a public Dashboard.

```text
# Anonymous (expect gated)
curl -sI https://services.example.test/
#   -> 3xx to /login, or login HTML body; NOT Dashboard tiles

curl -sI https://services.example.test/api/v1/domains
#   -> 401

curl -sf https://services.example.test/health
#   -> 200 (public probe path)

# Apex / www (packaged SurmountSystems/site; not the console)
curl -s https://example.test/ | grep -F 'Grok OSS'
curl -s https://www.example.test/ | grep -F 'Grok OSS'

# Operator: browser NIP-07 on https://services.../login with allowlisted npub
# Or NIP-98 kind 27235 against POST /api/v1/auth/session (see /login copy)
```

| Check | Pass |
|-------|------|
| Anonymous services `/` | Login redirect or login page; no operator console |
| Anonymous `/api/v1/domains` | **401** |
| `/health` | **200** without credentials |
| Allowlisted npub | Session after NIP-07 / NIP-98; console usable |
| Apex / www | Packaged SurmountSystems/site (not console) |

**Interim only** if Nostr material is not ready the same day: temporary
network restriction on services :443 or non-public console bind for
**hours-not-weeks**. That is ops bridge, not product end state. Dual-pin
residual if used. Product end state remains B4 Nostr on the public edge.

Closing B4 in residual requires **live proof** above, not docs alone.
**This host (2026-08-12):** anonymous services console is gated; Q-AUTH-1
stays open. Report: `.agents/reports/impl-auth-live-b4-switch.md`.

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
| `hardening.nix` | SSH (ed25519 host key only; leftover live RSA until deploy), fail2ban |
| `backups.nix` | restic |
| `networking.nix` | firewall |
| `web.nix` | edge (nginx transitional-to-delete; Rust target) |
| `mail.nix` | Stalwart + import helper |
| `management-ui.nix` | UI service |
