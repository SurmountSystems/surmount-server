# Mail migration: Synology MailPlus -> Stalwart

Goal: **lossless recovery of message content** with Maildir as the source of
truth. We do **not** attempt binary compatibility with Synology's SQLite or
MailPlus internal databases.

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

`<uid>` is the MailPlus / system user id directory, not always the email
local-part. Map uid -> address from MailPlus admin UI or account export
before import.

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

## Import with stalwart-cli (maildir-nested)

This repo installs a wrapper:

```bash
surmount-mail-import-maildir you@surmount.systems \
  /var/lib/surmount/import/maildir/<uid>/<uid>/Maildir
```

Equivalent manual form:

```bash
export STALWART_URL="http://127.0.0.1:8080"
# Auth flags as required by your Stalwart version / admin credentials:
#   stalwart-cli --url "$STALWART_URL" -u admin -p ... \
stalwart-cli --url "$STALWART_URL" import messages \
  --format maildir-nested \
  you@surmount.systems \
  /path/to/Maildir
```

Notes:

- **`maildir-nested`** matches Maildir++ folder layout (`.`-prefixed folders).
- If a tree is flat Maildir only, try `--format maildir` per upstream docs.
- Re-running import may **duplicate** messages depending on version; take a
  Stalwart data backup (or VM snapshot) before large imports.
- Import is **operator-started**. Nothing in activationScripts mutates mail.

## Verification checklist

- [ ] Message counts roughly match MailPlus (allow small deltas for junk)
- [ ] Spot-check threads with attachments and HTML
- [ ] Sent / Drafts / custom folders present
- [ ] IMAP login from a client (Thunderbird / Apple Mail)
- [ ] Send a test message out and receive a reply (after DNS/SPF/DKIM)

## Secondary formats

### MBOX

If you only have `.mbox` files:

```bash
stalwart-cli --url "$STALWART_URL" import messages \
  --format mbox \
  you@surmount.systems \
  /path/to/folder.mbox
```

(Confirm `--format` names against `stalwart-cli import messages --help` on
the installed package version.)

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
