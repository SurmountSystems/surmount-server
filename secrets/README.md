# Secrets (sops-nix) - deploy secrets

**Never commit secrets to git.** Not plaintext. Not ciphertext. Not age
private keys. Not LUKS unlock material. This is a **public** tree.

See standing law: [docs/hygiene.md](../docs/hygiene.md) (top rule),
[docs/SECRETS.md](../docs/SECRETS.md).

**Git admission:** `script/check-private-data.sh` (pre-commit + CI) blocks
common secret basenames and content shapes. Do not add `secrets.yaml`, PEMs,
or age keys here for commit. Detail: [docs/hygiene.md](../docs/hygiene.md)
*Pre-commit private-data scan*.

This directory is a **placeholder for layout documentation and empty
scaffolding only**. Real secret files live **on the host** (or a private
operator channel), never in the committed tree.

Surmount splits secrets into **two buckets** (plus optional disk encryption).
This folder documents **bucket 1** only: **deploy secrets** available at
NixOS activation.

| Bucket | Tool | Doc |
|--------|------|-----|
| **1. Deploy secrets** | **sops-nix** today (host-local material) | You are here; [docs/SECRETS.md](../docs/SECRETS.md) |
| **2. Human vault** | **Vaultwarden** (planned self-hosted) | [docs/SECRETS.md](../docs/SECRETS.md), [SECURITY.md](../docs/SECURITY.md) |
| Disk at rest (optional third) | **LUKS2** FDE first-class | [docs/SECURITY.md](../docs/SECURITY.md) |

Design notes: [docs/open-choices.md](../docs/open-choices.md),
[docs/operator-direction.md](../docs/operator-direction.md).
Data plane inventory (where secrets land on disk): [docs/DATASTORES.md](../docs/DATASTORES.md).

**Vaultwarden does not replace deploy secrets.** Machine secrets must exist
before Vaultwarden starts; flake eval stays pure; hermetic deploys cannot
depend on a live vault API. Upstream:
[dani-garcia/vaultwarden](https://github.com/dani-garcia/vaultwarden/).

**Why bucket 1 exists even if you prefer Vaultwarden:** Nix activation needs
secrets on disk before/during `nixos-rebuild switch` so systemd can start
services. sops-nix is the current scaffold tool; the *need* remains if the
tool changes. See SECRETS.md FAQ.

**agenix:** not dual-stacked with sops.

**LUKS:** unlock with passphrase / initrd SSH / TPM. Never put LUKS keyfiles
or recovery material in this repo (plain or encrypted).
[docs/research/luks2-and-deploy-secrets.md](../docs/research/luks2-and-deploy-secrets.md).

## Layout

```
secrets/
  README.md          # this file (safe to commit)
  .gitkeep           # empty tree marker only
```

Do **not** add `secrets.yaml`, keyfiles, or age private keys here for commit.
If you create encrypted files for practice on a workstation, keep them
**outside** this public repository (private path, host, or operator vault).

On the **host**, a typical layout might be:

```
/var/lib/sops-nix/key.txt                    # host age identity (never git)
/run/surmount-secrets/tls/cert.pem           # example path only (never git)
/run/surmount-secrets/tls/key.pem            # example path only (never git)
/run/surmount-secrets/arti/onion-service/    # Arti HS identity (never git);
                                             # MUST be owned/writable by
                                             # surmount-arti:surmount-arti
                                             # (e.g. 0750). Module does not
                                             # auto-create; ConditionPathIsDirectory
                                             # gates surmount-arti-hidden-service
/var/lib/surmount/secrets/...                # other host-local material
```

Point `sops.defaultSopsFile` (or equivalent) at a **host path** the operator
placed, not at a path that is tracked in the public flake tree.

## Fail loud when required material is missing

Module options (`modules/options.nix`, `modules/secrets.nix`):

| Option | Role |
|--------|------|
| `surmount.secrets.requireDeployMaterial` | When true, activation fails if required paths are missing |
| `surmount.secrets.requiredHostPaths` | List of `{ path; kind; }` (`kind` = `file` or `directory`) |
| `surmount.secrets.deployMaterialDir` | Conventional root (`/run/surmount-secrets`) |

Each `requiredHostPaths` entry is checked with `-f` (file) or `-d` (directory).
Mix PEM files and Arti state dirs in one list; there is no global kind and no
weak `any` default.

Example:

```nix
surmount.secrets.requireDeployMaterial = true;
surmount.secrets.requiredHostPaths = [
  { path = "/run/surmount-secrets/tls/cert.pem"; kind = "file"; }
  { path = "/run/surmount-secrets/tls/key.pem"; kind = "file"; }
  { path = "/run/surmount-secrets/arti/onion-service"; kind = "directory"; }
];
```

Production lean: set `requireDeployMaterial = true` and list every path the
stack needs. Scaffold and VM smoke keep the default **false** so eval works
without real secrets.

Hermetic pure contract tests: `tests/deploy-secrets.nix` (no secret files).

**Never** satisfy missing paths by committing material into this repo.

## Bootstrap (operator workstation + host)

1. Install tools (dev shell): `nix develop` (includes `sops`, `age`, `ssh-to-age`).
2. Create an age identity (workstation; **private; never commit**):

   ```bash
   mkdir -p ~/.config/sops/age
   age-keygen -o ~/.config/sops/age/keys.txt
   ```

3. Get the server host age key from its SSH host key (after first boot):

   ```bash
   ssh-to-age < /etc/ssh/ssh_host_ed25519_key.pub
   ```

4. Keep any `.sops.yaml` creation rules and encrypted payloads on the
   **operator machine or host**, not in the public repo. If a root `.sops.yaml`
   ever appears in-tree, it must contain **only** non-secret recipients/rules
   with no secret material; prefer keeping the whole sops workflow off-tree
   until a private-only ops path is designed.

5. Create secret files **outside git**, encrypt if you use sops locally, then
   **install onto the host** out-of-band (scp, nixos-anywhere secrets howto,
   console paste, etc.).

6. On the host, ensure decrypt key is available:
   - `/var/lib/sops-nix/key.txt`, or
   - SSH host key path listed in `modules/secrets.nix`.

7. Point `modules/secrets.nix` / host config at the **host-local** secrets
   file path and declare `sops.secrets.*`.

## Example secret keys (names only)

| Key | Purpose |
|-----|---------|
| `mail/admin_password` | Stalwart admin |
| `mail/accounts/admin` | Account password |
| `backups/restic_password` | restic repo |
| `vaultwarden/admin_token` | Vaultwarden admin (when enabled) |

Wire paths into `services.stalwart.credentials`,
`surmount.backups.passwordFile`, and (later) Vaultwarden env/secret files as
shown in module comments.

## Offline recovery (do not keep only on the mail disk; never in git)

- Workstation age identity backup
- LUKS recovery / passphrase (if FDE enabled; see SECURITY.md)
- restic password
- Vaultwarden emergency / admin materials (once VW exists)

## Related

- [docs/hygiene.md](../docs/hygiene.md) - NEVER secrets in git
- [docs/SECRETS.md](../docs/SECRETS.md) - full three-layer story
- [docs/SECURITY.md](../docs/SECURITY.md) - threat model, FDE, Nostr auth notes
- [docs/OPS.md](../docs/OPS.md) - backup posture
- `modules/secrets.nix` - NixOS wiring
