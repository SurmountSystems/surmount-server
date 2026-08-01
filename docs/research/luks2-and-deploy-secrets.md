# Research: sops-nix vs LUKS2 (deploy secrets and disk unlock)

**Status:** research finding + recommended single-VPS patterns.
**Date:** 2026-07-30 (hygiene correction same day: never secrets in git)
**Not** operator-accepted product law unless stated in operator-direction.

Living maps: [SECRETS.md](../SECRETS.md), [SECURITY.md](../SECURITY.md),
[hygiene.md](../hygiene.md) (top rule), [operator-direction.md](../operator-direction.md).

---

## Absolute: never unlock material in git

**Operator hygiene correction 2026-07-30.** This repo is public.

| Never in git | Why |
|--------------|-----|
| Plaintext LUKS passphrase or keyfile | Obvious |
| sops/age **ciphertext** of a LUKS keyfile or recovery key | Public repo + ciphertext of unlock material is still wrong; do not brainstorm it |
| Age private keys that decrypt production | Same class |
| Any blob whose only job is unlocking this host's disk | Stays off git, period |

Unlock path for root FDE: **passphrase** (console or initrd SSH), and/or
**TPM** when the hardware is real and controlled. Not a keyfile shipped from
this repository in any encoding.

Deploy secrets (restic password, service tokens, etc.) are a **different**
layer. They also **must not** live in the public git tree (plain or
ciphertext). They arrive on the host out-of-band. See [SECRETS.md](../SECRETS.md).

---

## Direct answer

**Does sops-nix unlock or configure LUKS2?**

**No.** sops-nix does **not** unlock LUKS2, does not replace cryptsetup, and
does not run in place of initrd unlock for an encrypted root.

| Layer | What it is | When it runs | Tool |
|-------|------------|--------------|------|
| **LUKS2** | Block-device full disk encryption (FDE) | **Initrd** (before real root is mounted) | `cryptsetup` / `boot.initrd.luks.*` / disko |
| **Deploy secrets (sops-nix scaffold)** | Decrypt service secrets to paths on the host | **NixOS activation** after root FS is up | age/gpg keys **on the host** (not from public git) |

They solve different threats. Do not mash them.

---

## What deploy secrets (sops-nix scaffold) actually do

From upstream sops-nix (Mic92/sops-nix), adapted to Surmount hygiene:

1. **Surmount rule:** secret material is **not** stored in the public repo.
   Ciphertext files, if used at all, live on the host or a private operator
   channel the host can read. Flake pure eval must not require live vault
   HTTP or plaintext secrets.
2. On the host, at **activation** (`nixos-rebuild switch` / activation
   scripts, or a sops-nix service path), sops decrypts using:
   - **age** private key on host (common: `/var/lib/sops-nix/key.txt`), and/or
   - **SSH host key** conversion (`sshKeyPaths` -> age), and/or
   - GPG, and/or cloud KMS env (less used here).
3. Plaintext lands under paths like `/run/secrets/...` with declarative
   owner/mode so systemd units can start.

That is **deploy secrets**: restic password, Vaultwarden admin token, session
keys, mail bootstrap secrets. See [SECRETS.md](../SECRETS.md) bucket 1.

### Initrd limitation (upstream, plain English)

sops-nix **does not fully support initrd secrets**. Upstream reason: bootloader
install runs before sops-nix's activation hook, so material needed *inside*
initrd is a special case with awkward workarounds. That is another signal that
sops-nix is **not** the LUKS root unlock path.

Even if someone designed custom initrd age decrypt later: **still never put
the ciphertext or keys in this public git tree.**

---

## What LUKS2 actually does

LUKS2 encrypts a **block device**. Until the volume is unlocked, the root
filesystem (and usually the age key that deploy-secret decrypt needs) is
ciphertext.

Typical NixOS path:

```text
  firmware / bootloader
       |
       v
  initrd
       |
       +--> network (optional: dropbear/SSH in initrd)
       +--> cryptsetup luksOpen  (passphrase | TPM | Tang/Clevis; not git keyfile)
       |
       v
  mount real root
       |
       v
  multi-user / NixOS activation
       |
       +--> deploy-secret decrypt --> /run/secrets/*
       +--> start stalwart, edge, vaultwarden, ...
```

Unlock mechanisms (NixOS wiki: Full Disk Encryption, Remote disk unlocking):

| Method | Unattended reboot? | Notes for a single VPS |
|--------|--------------------|------------------------|
| Passphrase at console | No | Fine if you have provider console |
| Passphrase via **initrd SSH** | No (human after reboot) | **Recommended default** for headless VPS |
| TPM2 enroll | Often yes on bare metal | Rare/absent on VPS; PCR fragility |
| Tang/Clevis network unlock | Yes if Tang up | Extra infra; network trust story |
| Keyfile on unencrypted `/boot` | Yes | **Weaker**; key sits next to ciphertext. Not default. Never source that keyfile from git. |
| Provider "encrypted volume" | Varies | Optional extra; not a substitute for OS LUKS we control |

**Scrubbed anti-pattern (do not revive):** "store a LUKS keyfile as sops
ciphertext in the repo." **Forbidden.** Not a research option for this
public tree.

---

## Chicken-and-egg (be honest)

**Root filesystem FDE + fully automated reboot without human, TPM, or network
unlock is a hard problem.** Something must open the LUKS volume before the OS
that holds deploy-secret keys can run.

| Want | Reality |
|------|---------|
| Unlock root with material from public git | **No.** Never. |
| Decrypt a keyfile with sops-nix **on the encrypted root** | **Too late** for unlocking that same root |
| Decrypt keyfile in **initrd** via sops | Not what sops-nix is built for; and still no git storage |
| Automated reboot after crash with FDE | Passphrase-at-console, initrd SSH human, TPM, or Tang. Not a git-backed keyfile. |

Age key material used by sops-nix typically lives on the root FS
(`/var/lib/sops-nix/key.txt` or SSH host keys). That material is only available
**after** LUKS unlock. Impermanence setups must put that key on a **persisted
path available early enough after root is up**; that is still not initrd LUKS
unlock. Host age keys are never committed.

---

## Parsimonious recommended patterns (single VPS)

### Option 1 (recommended default): passphrase + initrd SSH

- Install with **disko + LUKS2 root** (nixos-anywhere or equivalent) when the
  provider path allows.
- Unlock: operator enters passphrase via **provider console** or **SSH into
  initrd** after reboot.
- Keep recovery passphrase **offline** (paper / USB / human vault that is not
  only on this disk, and **not** in git). Vaultwarden on the same host is not
  enough as sole recovery.
- After unlock, normal multi-user boot; deploy secrets decrypt on host as
  configured.

**Pros:** clear threat model; well documented on NixOS; no key-next-to-disk;
no secrets in repo.
**Cons:** every reboot needs a human (or you accept downtime until someone
unlocks). Honest tradeoff for a single mail VPS.

### Option 2: keyfile on small unencrypted boot partition (weaker; not default)

- `/boot` (or ESP + boot) unencrypted; keyfile used by initrd for root LUKS.
- Automated reboot works.
- Keyfile is created **on the host at install time** (or uploaded out-of-band
  at install). **Never** checked into git, plain or encrypted.

**Pros:** unattended reboot.
**Cons:** **weaker**. Anyone with the full disk image often has boot + keyfile
+ ciphertext. Use only if unattended reboot is a hard requirement and residual
risk is accepted in writing.

### Option 3: deploy secrets only after root is unlocked

- Service secrets (restic, VW admin, session keys) decrypt at activation.
- Material and keys stay on host / private operator channels.
- **Does not unlock root.** Do not advertise this as "sops handles FDE."

### What we are not recommending

- **Any** LUKS unlock material in the public git tree (plain or ciphertext).
- Tang/Clevis solely for one VPS as Day-1 default (ops weight).
- TPM2 on a generic cloud VPS without evidence the provider exposes a useful
  TPM you control.
- Custom "age decrypt in initrd from repo ciphertext" (**Q-LUKS-3** closed
  lean: no; and git storage is forbidden anyway).

---

## How the layers fit Surmount language

```text
  Two buckets + disk (not a third "secrets product"):

  Bucket 1  Deploy secrets     on-host decrypt at activation (sops-nix scaffold)
  Bucket 2  Human vault        Vaultwarden after host is up
  Disk      LUKS2 FDE          initrd unlock (passphrase / initrd SSH / TPM);
                               unlock material NEVER in git
```

| Question | Answer |
|----------|--------|
| Does sops replace LUKS? | No |
| Does LUKS replace sops? | No |
| Does Vaultwarden unlock LUKS or feed nixos-rebuild? | No |
| Secrets in public git? | **Never** (plain or ciphertext) |
| Order at boot | LUKS unlock -> root -> deploy decrypt -> services (incl. VW) |

---

## Open questions

**Q-LUKS-1.** After the provider answers, is first install LUKS2 from day one,
or interim plain disk then planned reinstall? (Also **Q-HOST-2**.)

**Q-LUKS-2.** Accept human unlock after every reboot (Option 1), or accept
weaker host-local keyfile-on-boot for unattended reboot (Option 2)? Keyfile
still never from git.

**Q-LUKS-3.** Custom initrd age decrypt later? Default lean: **no**. Git
storage of unlock material remains forbidden either way.

**Q-LUKS-4.** Encrypt only data partitions vs full root? Full root preferred
when install path allows; data-only is a weaker interim.

---

## Related

- [hygiene.md](../hygiene.md) - top rule NEVER secrets in git
- [SECRETS.md](../SECRETS.md) - buckets and anti-patterns
- [SECURITY.md](../SECURITY.md) - LUKS section and threat model
- [operator-direction.md](../operator-direction.md) - host + secrets direction
- Upstream: https://github.com/Mic92/sops-nix (initrd limitations in README)
- NixOS wiki: Full Disk Encryption; Remote disk unlocking
