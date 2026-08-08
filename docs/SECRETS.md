# Secrets architecture (two buckets + optional disk encryption)

How Surmount handles secrets without mashing deploy crypto, human password
managers, and disk encryption into one vague bucket.

**Last updated:** 2026-07-30 (hygiene: never secrets in git)
**Operator direction:** [operator-direction.md](operator-direction.md)
**Companions:** [hygiene.md](hygiene.md) (top rule), [SECURITY.md](SECURITY.md),
[DATASTORES.md](DATASTORES.md), [glossary.md](glossary.md),
[`secrets/README.md`](../secrets/README.md), `modules/secrets.nix`.

**Language:** prefer **encrypted deploy secrets** or **secrets available at
NixOS activation**. Avoid vague "seal" jargon.

---

## 0. NEVER secrets in git (absolute)

**Public repo. Zero secret material in the tree.**

| Forbidden | Notes |
|-----------|--------|
| Plaintext passwords, tokens, private keys, `.env` | Obvious |
| sops/age **ciphertext** of production secrets | Not "safe because encrypted" in a public repo |
| LUKS keyfiles, passphrases, recovery keys (any encoding) | Unlock material never in git |
| Age/sops **private** keys | Admin and host identities stay off git |

**Do not suggest** committing secrets "encrypted for safety," including
"sops-encrypted LUKS keyfile in git." That pattern is **entirely out of the
question**.

**Correct model:** secrets stay off git. Deploy secrets live on the host via
operator-controlled channels (out-of-band copy, install-time upload, private
operator machine, local age keys never pushed). Public tree has configs and
modules only.

Detail: [hygiene.md](hygiene.md) top rule;
[research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

**Pre-commit / CI gate:** `script/check-private-data.sh` scans staged (and in
CI, tracked) files for private-data **pattern classes** via ripgrep. Patterns
only; no real secrets live in the tool. Project hook:
`script/git-hooks/pre-commit`. See [hygiene.md](hygiene.md) subsection
*Pre-commit private-data scan*.

---

## 1. Two buckets (do not mash)

| Bucket | Name | Tool (today / direction) | Job | Not the job of |
|--------|------|--------------------------|-----|----------------|
| **1** | **Deploy secrets** (machine / config) | **sops-nix** + age (scaffold; *need* required even if tool changes) | Decrypt service secrets **on the host** at activation into restricted paths so systemd can start services | Human day-to-day password UX; FDE; git storage |
| **2** | **Vaultwarden** (human / org vault) | **Vaultwarden** (planned, self-hosted) | Operator passwords, TOTP, secure notes, shared team items, mail credential inventory UX | Feeding `nixos-rebuild`; encrypting mail RocksDB |

Upstream Vaultwarden: https://github.com/dani-garcia/vaultwarden/

### Optional third: disk encryption (not a secrets "bucket" peer)

| Concern | Tool | Job |
|---------|------|-----|
| Disk at rest | **LUKS2** (first-class on the VPS) | Offline disk / snapshot protection |

LUKS passphrase/unlock is **disk encryption**. It is not deploy-secret
distribution and not a human password manager. Keep it in the threat model
([SECURITY.md](SECURITY.md)); do not fold it into Vaultwarden or sops as if
they were the same product.

**sops-nix does not unlock or configure LUKS2.** Deploy-secret decrypt runs at
NixOS **activation** (after root is mounted). LUKS2 unlock happens in
**initrd**, before that. Unlock: passphrase / initrd SSH / TPM. **Never** put
LUKS unlock material in git. Full patterns:
[research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

If a design needs Bitwarden to unlock the root filesystem before Vaultwarden
can start, the layers were mashed. Stop and redesign.

---

## 2. Why deploy secrets are not Vaultwarden (operator FAQ)

The operator is right to question stacking tools. The split is **timing and
trust domain**, not brand loyalty.

```text
  nixos-rebuild / activation
       |
       v
  Need: restic password, VW admin token, session keys, bootstrap secrets
       |
       |  Cannot call Vaultwarden over HTTP here:
       |  VW is not up yet, and flake eval must stay pure.
       v
  Bucket 1: deploy secrets on host + host age/SSH key material
  (secrets available at NixOS activation; NOT from public git)
       |
       +--> starts stalwart, restic, vaultwarden, edge, ...
       v
  Humans use Vaultwarden (bucket 2) for day-to-day secrets
```

| Concern | Deploy secrets (1) | Vaultwarden (2) |
|---------|--------------------|-----------------|
| Who uses it | Nix activation, systemd units | Humans, browsers, bitwarden clients |
| Must work offline at boot | Yes | No (after network + VW up) |
| Pure flake eval | No secret material in public tree | Must not be required at eval |
| In public git? | **Never** | Server data on host disk; not the deploy channel |
| Mail app passwords UX | Not the primary UI | Yes: track/share/rotate for humans |
| Nostr nsec | Never on server | Never casually in shared vault either |

**Vaultwarden does not replace deploy secrets.**
**Deploy secrets do not replace Vaultwarden for humans.**

### sops-nix specifically

- **Current scaffold:** sops-nix is wired as the deploy-secrets *mechanism*
  (`modules/secrets.nix`). The tool decrypts on the host; it is **not**
  permission to store secret ciphertext in the public repo.
- **Operator may question sops** as the long-term tool. That is fair.
- **What cannot go away:** *some* way to make machine secrets available at
  activation without a running human vault (sops-nix, or a deliberate
  alternative evaluated later), with material supplied **out of band**.
- **agenix:** not dual-stacked with sops. A full *replacement* of sops-nix is
  a different decision from "also run agenix."
- Open: **Q-DEP-1** in [open-choices.md](open-choices.md) (keep sops-nix vs
  evaluate alternative tool; the *need* stays; git storage stays forbidden).

---

## 3. Bucket 1: deploy secrets on the host (sops-nix scaffold today)

### Rules

1. **Never secrets in git** (plain or ciphertext). See section 0.
2. **One deploy-secrets system at a time.** Do not dual-run agenix + sops.
3. **No plaintext** passwords, tokens, or private keys in `flake.nix`, modules,
   or world-readable Nix store paths.
4. Flake **pure eval** must not require live vault HTTP or unlocked human vault.
5. Decrypt **on the host** via key material the host already has (age key
   and/or SSH host key derivation as configured).
6. Secret *files* the host reads come from operator-controlled paths (for
   example under `/var/lib/...` or a host-local sops file the operator placed),
   not from a committed `secrets/*.yaml` in the public tree.

### Host key material (sops-nix defaults)

| Item | Default / convention |
|------|----------------------|
| Age key file | `/var/lib/sops-nix/key.txt` (`modules/secrets.nix`) |
| Optional | Derive from SSH host key via `sshKeyPaths` (`/etc/ssh/ssh_host_ed25519_key`) |
| Secret files | Host-local only; never committed. See [`secrets/README.md`](../secrets/README.md). |

Bootstrap steps: [`secrets/README.md`](../secrets/README.md).

### Typical secret keys (names only)

| Key | Consumer |
|-----|----------|
| `mail/admin_password` | Stalwart admin bootstrap |
| `mail/accounts/*` | Account bootstrap if declarative |
| `backups/restic_password` | `surmount.backups.passwordFile` |
| `vaultwarden/admin_token` | Planned VW admin |
| `vaultwarden/database_url` or DB password | If VW not on default SQLite path |
| Session / cookie signing keys | `SURMOUNT_SESSION_SECRET` (host deploy secret; HMAC cookie scaffold) |

Wire with `sops.secrets.<name>.path` and unit `LoadCredential` / env files as
modules document. Prefer path references over embedding secret values in
generated world-readable config when the module allows credentials macros.

### Offline recovery (deploy secrets)

- Keep age identities for admin machines **offline-capable** (encrypted USB,
  paper backup of age secret, etc.). **Not** in git.
- Losing only the server age key may be recoverable if SSH host key path is
  still enabled and host keys exist; do not rely on one copy.
- Host ciphertext without any decrypt key is worthless; treat host backup of
  secret files as operator responsibility.

---

## 4. Bucket 2: Vaultwarden (human vault)

### Intent

- Self-hosted Bitwarden-compatible API for humans and teams.
- Reverse-proxied on a **private** hostname (UDS behind Axum-first edge preferred);
  **no public signup**.
- Admin token and any DB password come from **deploy secrets**.
- **Mail credentials** live in Stalwart; Vaultwarden is how humans
  track/integrate those secrets (app passwords, recovery notes), not the
  engine's directory store.

### Data store choice (see DATASTORES.md)

| Engine | Recommendation |
|--------|----------------|
| **SQLite** | **Default for single VPS** |
| PostgreSQL | Only if already running PG or multi-instance VW |

Data directory must be on the restic path list. Client-side encryption protects
vault item payloads; the server DB and attachments still need backup and disk
hygiene.

### Chicken-and-egg

```text
  nixos-rebuild
       |
       v
  deploy secrets decrypt (age on host) --> /run/secrets/...
       |
       +--> stalwart-mail
       +--> restic
       +--> vaultwarden  (starts with deploy-secret admin/DB material)
       |
       v
  humans use Vaultwarden UI/API  (never required at eval time)
```

### What Vaultwarden is not

- Not a replacement for encrypted deploy secrets (sops-nix or alternative).
- Not encryption for Stalwart/RocksDB mail.
- Not LUKS unlock.
- Not Nostr nsec storage (never put nsec in a shared vault casually; prefer
  hardware/client wallets and host OS key tools; if stored, highest sensitivity).
- **Not Bitwarden Secrets Manager.** Bitwarden Secrets Manager is a separate
  cloud product (`bws`, SM SDK, service accounts). Vaultwarden implements the
  **password manager** client API. Maintainers have stated SM is a licensed
  feature VW does not implement. Do not document "VW Secrets Manager API" as
  if it existed. For **Arti** (required onion/HS) and other services: startup
  material (including HS identity keys) stays **deploy secrets** on the host
  (**never in git**); human inventory and optional `bw` CLI workflows use VW
  as a password vault. Detail:
  [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md),
  [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7.

---

## 5. Disk: LUKS2 FDE (optional third concern)

Full posture: [SECURITY.md](SECURITY.md).
Deep research: [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).
Operator direction: **first-class** consideration for the VPS.

Short form:

- Prefer LUKS2 root on greenfield/reinstall (disko + nixos-anywhere).
- Headless VPS unlock: **initrd SSH + passphrase** is the realistic default
  (Option 1). Host-local keyfile on unencrypted boot works unattended but is
  weaker (Option 2); keyfile is never from git. Deploy secrets after root
  unlock are too late for root LUKS (Option 3).
- **Honest chicken-and-egg:** automated reboot with root FDE needs human,
  TPM, network unlock (Tang), or a weaker keyfile-beside-disk pattern on the
  host. sops-nix is none of those for root. Git is never the unlock channel.
- FDE protects **offline** theft; not live root compromise.
- LUKS recovery material must exist **off** the encrypted disk, **off git**,
  and not only inside Vaultwarden that lives on that disk.

---

## 6. Product auth secrets (Nostr)

Product login is Nostr keys (npub + NIP-98 and sessions). Keys managed by host
OS via other Surmount tools; dead simple; not manual password management for
users. See operator-direction and SECURITY.

| Material | Where |
|----------|-------|
| **nsec** (private) | Client / OS key tools only; **never** server database; **never** git |
| npub allowlist / role map | `SURMOUNT_NOSTR_ALLOWLIST` or `SURMOUNT_NOSTR_ALLOWLIST_FILE` / Nix `managementUi.nostrAllowlist` + `nostrAllowlistFile` (scaffold; env wins; Q-AUTH-1 bootstrap UX open) |
| Session signing keys / cookie secrets | Host-only `SURMOUNT_SESSION_SECRET` or EnvironmentFile via `managementUi.sessionSecretPath` (HMAC cookie scaffold; never git) |
| Stalwart management API token (directory list) | Host-only `SURMOUNT_STALWART_TOKEN` or raw file `SURMOUNT_STALWART_TOKEN_FILE` / Nix `managementUi.stalwartTokenPath` (Bearer for management JMAP; never git). Required only when `directory=stalwart` |
| Stalwart mail passwords / app passwords | Inside Stalwart directory (engine); human tracking in Vaultwarden |

---

## 7. Backup checklist for secrets-related material

| Item | Backup? | Notes |
|------|---------|-------|
| Public git tree | Via git remote | Must contain **no** secrets |
| Host deploy-secret files | Host backup / operator channel | Not the public repo |
| age identities | **Offline** copies | Critical; never git |
| SSH host keys | Careful backup / documented regeneration | Affects ssh-to-age |
| restic password | deploy secrets + offline | Without it backups are gone |
| restic repo | Provider durability + second location ideal | Encrypted |
| Vaultwarden data | restic | Critical when VW live |
| LUKS passphrase/header | Offline only | Highest; never git |
| ACME keys | Optional | Re-issuable if DNS/HTTP-01 works |
| Stalwart DB | restic of mail data dir | Contains credential hashes |

Clever RPO/RTO design is **later, not now** (operator direction). Basic
restic still worth enabling before real mail.

---

## 8. Anti-patterns

| Don't | Do |
|-------|-----|
| Commit secrets plain or ciphertext | Keep secrets off git; host/out-of-band only |
| Suggest sops-encrypted LUKS keyfile in git | Passphrase / initrd SSH / TPM |
| Put secrets in `flake.nix` attrs as plaintext | Secret paths after activation decrypt |
| Expect VW to unlock before deploy secrets | Deploy secrets first |
| Store nsec on server "for convenience" | Client/OS-held keys only |
| Only copy of age key on the mail VPS disk | Offline admin copy (not git) |
| Dual sops + agenix | One deploy-secrets system only |
| restic password only inside VW on same host | Offline + deploy secrets |
| Claim VW replaces sops | Document both jobs |

---

## 9. Related modules

| Path | Role |
|------|------|
| `modules/secrets.nix` | sops-nix defaults |
| `modules/backups.nix` | restic password file hook |
| `modules/mail.nix` | Stalwart credentials comments |
| `secrets/README.md` | Operator bootstrap (host-local; nothing secret committed) |
