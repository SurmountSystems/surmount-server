# Mail migration: Synology MailPlus -> Stalwart

**Gated residual (2026-08-12):** this track does **not** start until the
DiskStation is reachable on the LAN (or the operator gives a reachability
path). Engine dual-sign register (Track A) should be sign-ready before a
test-account import. NAS stays read-only until counts look right.

Goal: **lossless recovery of message content** with Maildir as the source of
truth. We do **not** attempt binary compatibility with Synology's SQLite or
MailPlus internal databases.

Old Synology vhosts that come with the NAS cutover are **static HTML only**
(no PHP/Node leftovers). Serve them from apex/www only after UNDER
CONSTRUCTION is no longer the product page. Inventory hosts/paths when you
can see the NAS; do not invent a CMS.

## Priority order

1. **Primary:** nested Maildir trees (MailPlus local accounts)
2. **Secondary:** MBOX exports (if you have them)
3. **Last resort:** PST/OST via conversion tools (Outlook leftovers)
4. **Out of scope:** reverse-engineering MailPlus SQLite schemas

## Where MailPlus keeps Maildir

On a DiskStation (e.g. DS3018xs) the usual layout looks like:

```text
/volume1/MailPlus/Maildir/@local/<uid>/<uid>/Maildir/
  cur/
  new/
  tmp/
  .Sent/
  .Drafts/
  .INBOX.Archive/    # Maildir++ style nested folders
  ...
```

Exact volume name and path prefix vary (`/volume1`, `/volume2`, shared folder
rename). Find a directory that contains `cur/`, `new/`, and `tmp/`  -  that is
the Maildir root to import.

**AFP share (laptop GVFS, verified 2026-08-13):** share name `MailPlus`.
Mailbox trees are `MailPlus/@local/<uid>/<uid>/Maildir/{cur,new,tmp,.Sent,...}`.
The `/volume1/MailPlus/Maildir/` prefix is an on-box example, not the GVFS
share root. Documented URI (mDNS hostname default; IPv4 allowed at runtime):
`afp://hunter@DS1513.local/MailPlus` or
`afp://hunter@DS3018xs.local/MailPlus`. There are two NAS units. Units are
already configured. **DSM rename is optional. mDNS is optional.** Last-week
path is gio by operator IPv4. Pass `--afp-host` or `--uri` with that address
at runtime; do not paste office LAN IPv4 into this tree. Optional
laptop-private hint (never git): `~/.local/share/surmount/diskstation-afp-hosts`
(`DS1513=<afp-host>`). Discover which one answers on the laptop LAN with
`just diskstation-discover` (Avahi plus `getent hosts` on those `.local`
names; no IPv4 printed) is still available, not required. Discover the
live GVFS directory (`*host=<id-or-ip>*volume=MailPlus*` under
`/run/user/$UID/gvfs`). libc `readdir` of
non-empty `cur/` I/O-errors; copy with `gio list` and reconstruct `:2,`
(`just mailplus-copy-uid -- --host DS1513 <uid>`). Do **not** use bash
`[[ -d cur ]]` on AFP: a non-empty `cur/` I/O-errors and bash treats that
as "not a directory", which listed **0** mail. `gio list` is the listing
path even when `cur/` looks missing to libc. Canonicalize GVFS with
`readlink -f` (a symlink wrapper makes `gio list` of `cur/` return empty).
`--host` prefers the laptop-private hint leaf first (bounded `test -e`)
so a hung GVFS root glob does not stall the copy.

`<uid>` is the MailPlus / DSM-style numeric directory, not the email
local-part. Paths have no username and no address. Map uid to mailbox from
MailPlus admin, DSM users, or the living operator pin
(`~/.agents/surmount-server/operator-facts.md` on the operator machine).
Do not guess from mailbox size. Do not paste that roster into this public
tree. Distinct MailPlus accounts are separate User mailboxes until the
operator says otherwise. The same local-part on another domain is not an
alias of an existing mailbox unless the operator says so. IMAP username is
that mailbox's own address. Extra mail domains may exist locally before
public MX points here. Do not import a uid the operator did not name.
SQLite reverse-engineering remains out of scope.

Laptop mount (GNOME Secret Service, never put the password on argv).
Host ids are **DS1513** (5-bay) and **DS3018xs** (6-bay). One Secret Service
item per NAS. Passwords may differ. Do not assume which unit holds MailPlus.

```bash
just diskstation-discover
just secrets-prompt -- synology-afp --host DS1513 --secret-service --no-staging
just secrets-prompt -- synology-afp --host DS1513 --afp-host <IPv4-or-hostname> --secret-service --no-staging
just secrets-prompt -- synology-afp --host DS3018xs --secret-service --no-staging
just diskstation-afp-mount -- --host DS1513
just diskstation-afp-mount -- --host DS1513 --afp-host <IPv4-or-hostname>
just diskstation-afp-mount -- --host DS3018xs --uri afp://hunter@<IPv4-or-hostname>/MailPlus
just mailplus-copy-uid -- --host DS1513 <uid>
```

Use `--host` for the Secret Service label (DS1513 or DS3018xs), not as a
required mDNS name. One-time paste into `secrets-prompt` also stores GNOME
NetworkPassword (`protocol=afp`, `user=hunter`, `server` = host id and
`<id>.local`). `--afp-host` / `--uri` / hint may be IPv4 for the mount URI
only; IPv4 is never stored as NetworkPassword `server`. After a successful
mount, missing NetworkPassword is filled from the same lookup (never printed,
never on argv). After every store, paste buffers are wiped (empty stdin).
DSM rename is still optional.
Import only from the unit that has Jeff's Maildir after
you name the numeric uid from MailPlus or DSM admin. `secrets-install-host`
refuses `synology-afp` (laptop-only). Unmount that host:
`just diskstation-afp-mount -- --host DS1513 unmount`.

DSM Control Panel rename to **DS1513** / **DS3018xs** is optional (mDNS
convenience only). Do not write last-week office LAN IPv4 into git.

### Copying off the NAS

Prefer a consistent snapshot:

```bash
# On NAS or via rsync over SSH  -  example only
rsync -aH --numeric-ids \
  /volume1/MailPlus/Maildir/@local/ \
  backup-host:/data/mailplus-export/@local/
```

Stage on the new server (not auto-imported):

```bash
# On mail-vps (sample host name; use your real SSH host)
mkdir -p /var/lib/surmount/import/maildir
rsync -a backup-host:/data/mailplus-export/@local/ \
  /var/lib/surmount/import/maildir/
```

## Create the Stalwart account first

Import attaches messages to an existing account. Create the principal via:

- Stalwart admin UI (SSH tunnel to `http://127.0.0.1:8080`), or
- `stalwart-cli` directory/account commands for your Stalwart version

Use the final production address, e.g. `you@surmount.systems`.

## Attach npub for services portal login

IMAP import does not grant portal login. After the mailbox exists, an
Administrator signs into `https://services.<apex>/mail` and uses
**Grant console login**: mailbox address, paste bech32 `npub1...`, role
User (default) or Administrator, Save. Session-bound CSRF is required.
That writes `/var/lib/surmount/console/accounts.json` so the person can
log in with AuthMode nostr. After they sign in at
`https://services.<apex>/login` (NIP-07), they set **their own** IMAP
password on `/mail`. The boss can still set a password as support.
Directory listing may stay unavailable.
Do not paste real npubs into this tree. User npubs stay in the map, not
the host allowlist. Q-AUTH-1 is unchanged.

## Import with Vandelay (Stalwart 0.16)

Live `stalwart-cli` **1.0.12** is schema-driven JMAP admin only (`get`,
`query`, `create`, `update`, `delete`, `describe`, `apply`, `snapshot`).
It has **no** `import` subcommand. Do not run `stalwart-cli import messages`.
The engine binary `stalwart -i` imports a **Stalwart store dump**, not
Maildir.

Official 0.16 Maildir++ path is **Vandelay** (pin `nix/packages/vandelay.nix`,
currently 1.0.7): local tree -> SQLite archive -> JMAP export.

See [Vandelay](https://github.com/stalwartlabs/vandelay) (accessed:
2026-08-13) and [Vandelay: the JMAP importer-exporter](https://stalw.art/blog/jmap-account-migration/)
(accessed: 2026-08-13). CLI overview (no import command):
[stalwart-cli](https://stalw.art/docs/management/cli/) (accessed: 2026-08-13).

This repo installs `vandelay` plus a wrapper:

```bash
# On the mail host, after the account exists and Maildir is staged.
# Token: STALWART_TOKEN or Domain B /var/lib/surmount/secrets/ui/stalwart-api-token
# JMAP: loopback http://127.0.0.1:8080  (/.well-known/jmap -> /jmap/session)
surmount-mail-import-maildir hunter@surmount.systems \
  /var/lib/surmount/import/maildir/1029/1029/Maildir
# Other uids (after create User, not Admin):
#   1039 -> jeff@surmount.systems  --account-id d
#   1038 -> kyle@surmount.systems  --account-id e
#   1030 -> ian@surmount.systems   --account-id f
#   1033 -> baxter@baxterartworks.com --account-id g
#           (aliases bax@cryptoquick.com, baxter@surmount.systems)
# Hunter aliases (same Account id c; do not create mailboxes):
#   1040 / 1037 / 1024 / 1031 -> hunter@surmount.systems --account-id c
#   archives hunter_alias_<uid>.sqlite
```

Live JMAP session `apiUrl` is `https://mail.surmount.systems/jmap/`.
That hostname is **not** on the current certificate. Vandelay export
must use the same loopback session rewrite hunter 1029 used (serve a
rewritten session whose `apiUrl` is `http://127.0.0.1:8080/jmap/`, then
`STALWART_URL=http://127.0.0.1:18080`). Do not hit public HTTPS for
import. Pass `--account-id` (this engine's `query Account --where
emailAddress=` is unsupported).

Equivalent manual form (same token env as the wrapper):

```bash
export STALWART_URL="http://127.0.0.1:8080"
export VANDELAY_TOKEN="$STALWART_TOKEN"

vandelay import maildir \
  /var/lib/surmount/import/maildir/1029/1029/Maildir \
  /var/lib/surmount/import/vandelay/hunter_surmount.systems.sqlite \
  --exclude 'All Mail'

# Live proof 2026-08-13: admin token Email/query works with Account id c
# (hunter). Session personal account is admin; pass --account-id, not the
# email as accountId.
vandelay export \
  --url "$STALWART_URL" \
  --auth-bearer \
  --account-id c \
  --objects mailbox,email \
  /var/lib/surmount/import/vandelay/hunter_surmount.systems.sqlite
```

Notes:

- **Maildir++** (`.`-prefixed folders) is what Vandelay `import maildir` reads.
- Default wrapper exclude is `All Mail` (MailPlus `.All Mail` Gmail catch-all).
- Re-runs are **convergent** in Vandelay (safe to resume). Still take a
  Stalwart data backup before the first large export. Exporting a small
  archive into hunter rematches the existing mailbox (Email/query pages
  of 500). Live 2026-08-20 alias exports took about 15-20 minutes of
  rematch each. Uid **1024** dest 325 matched mail already in hunter
  (JMAP `created=0 skipped=325`); uids **1040** / **1037** / **1031**
  created 3+1+1. Inbox `totalEmails` 88912 -> 88917.
- Import is **operator-started**. Nothing in activationScripts mutates mail.
- The wrapper is on PATH after deploy. Hermetic test:
  crate tests for `surmount-mail-import-maildir`.

## Verification checklist

- [ ] Message counts roughly match MailPlus (allow small deltas for junk)
- [ ] Spot-check threads with attachments and HTML
- [ ] Sent / Drafts / custom folders present
- [ ] IMAP login from Evolution (mail hostname 993 / 465, full address,
      Let's Encrypt, Normal password)
- [ ] Send a test message out and receive a reply (after DNS/SPF/DKIM)

## Secondary formats

### MBOX

If you only have `.mbox` files:

Convert `.mbox` to Maildir on a workstation (or use Vandelay
`import takeout` on a directory of `.mbox` files), then export to JMAP
the same way as Maildir. Do not call `stalwart-cli import`.

### PST

Convert with `readpst` / `libpst` to mbox or Maildir on a workstation, then
import the result. Do not install GUI-only converters on the mail VPS.

## What we deliberately skip

- Binary restore of Synology PostgreSQL/SQLite MailPlus DBs into Stalwart
- Perfect preservation of MailPlus-only metadata (labels that never landed
  in Maildir flags/keywords)
- Automatic cutover MX flip before import verification

## Cutover sequence (recommended)

1. Lower DNS TTLs (24-48h ahead).
2. Build the sample host (`#mail-vps`); get TLS, auth, and a test account working.
3. rsync Maildir; import into Stalwart; verify.
4. Final rsync + incremental import (or brief dual-receive window).
5. Flip MX / SPF / DKIM / DMARC (see `docs/DNS.md`).
6. Keep NAS read-only for a retention window, then decommission.

## Rollback

- Keep NAS online read-only until you trust the new host.
- restic backups of `/var/lib/stalwart-mail` once production mail lands
  (`surmount.backups`).
- MX can point back to the old host if it still accepts mail (plan this
  before decommission).
