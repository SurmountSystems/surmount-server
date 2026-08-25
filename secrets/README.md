# Secrets (sops-nix) - deploy secrets

**Never commit secrets to git.** Not plaintext. Not ciphertext. Not age
private keys. Not LUKS unlock material. This is a **public** tree.

See standing law: [docs/hygiene.md](../docs/hygiene.md) (top rule),
[docs/SECRETS.md](../docs/SECRETS.md).

**Git admission:** `surmount-private-data` (pre-commit + CI) blocks
common secret basenames and content shapes. Do not add `secrets.yaml`, PEMs,
or age keys here for commit. Detail: [docs/hygiene.md](../docs/hygiene.md)
*Pre-commit private-data scan*.

This directory is a **placeholder for layout documentation and empty
scaffolding only**. Real secret files live **on the host** (or a private
operator channel), never in the committed tree.

## Three custody domains (point here for day-to-day)

| Domain | Role | Doc |
|--------|------|-----|
| **A. Operator workstation** | GNOME Secret Service / private staging (SoT for operator custody) | [docs/SECRETS.md](../docs/SECRETS.md) section 1 |
| **B. Host deploy secrets** | Files at activation under e.g. `/run/surmount-secrets/...` | You are here for path conventions; install via bridge |
| **C. Human vault** | Vaultwarden (S7a module offline; S7b live residual; V4 human-store runbook); not activation feed | [docs/SECRETS.md](../docs/SECRETS.md) section 5 |

**Domain A intake (prefer this):** `just secrets-prompt` /
`cargo run -p surmount-secrets-prompt` prompts without echo and writes private
staging (optional `secret-tool`). **Stalwart unlock:** when the admin password
is unknown, prefer `just fix-public-dashboard -- --target root@HOST` (urandom
recovery pin + mint API key + free-443 dry-run; kind `stalwart-recovery-admin`).
When you have the first admin password, prefer `just bootstrap-stalwart-token`
(CLI mints key; WebUI optional). When you already hold the key string, prefer
`just add-stalwart-token` (pipe / `--token-file` / paste; defaults host
`surmount-1`; existing Domain A + `--target` installs without re-prompt).
Do **not** paste ApiKeys or tokens into chat or shell history. See
[docs/SECRETS.md](../docs/SECRETS.md) section 1.5 and
[docs/OPS.md](../docs/OPS.md) *Fix public dashboard* / *Bootstrap Stalwart API token*.

**Install bridge (S3 offline):** `nix run .#secrets-install-host` reads a
private **staging dir** (or optional `secret-tool`) and installs owner-only
files to host paths. Refuses public `hosts/` / `secrets/`. Hermetic test:
crate tests in `checks.*.ci`. **Not** a license to put secrets in
this git directory. Schema + runbook: [docs/SECRETS.md](../docs/SECRETS.md).

Surmount also splits product buckets (plus optional disk encryption). This
folder documents **bucket 1** only: **deploy secrets** available at NixOS
activation (domain B material).

| Bucket | Tool | Doc |
|--------|------|-----|
| **1. Deploy secrets** | **sops-nix** today and/or direct file install from domain A | You are here; [docs/SECRETS.md](../docs/SECRETS.md) |
| **2. Human vault** | **Vaultwarden** (S7a offline; S7b live residual; optional export to staging) | [docs/SECRETS.md](../docs/SECRETS.md) section 5, [SECURITY.md](../docs/SECURITY.md) |
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
/var/lib/surmount/secrets/                   # durable Domain B root (survives reboot)
/var/lib/surmount/secrets/ui/session-secret  # session HMAC (example; never git)
/var/lib/surmount/secrets/ui/stalwart-api-token
/var/lib/surmount/secrets/acme/namecheap.env # Namecheap DNS-01 env (never git)
/var/lib/surmount/secrets/vaultwarden/admin.env
/run/surmount-secrets/tls/cert.pem           # ephemeral PEM path example (never git)
/run/surmount-secrets/tls/key.pem            # ephemeral PEM path example (never git)
/run/surmount-secrets/arti/onion-service/    # Arti HS identity (never git);
                                             # MUST be owned/writable by
                                             # surmount-arti:surmount-arti
                                             # (e.g. 0750). Module does not
                                             # auto-create; ConditionPathIsDirectory
                                             # gates surmount-arti-hidden-service
```

Point `sops.defaultSopsFile` (or equivalent) at a **host path** the operator
placed, not at a path that is tracked in the public flake tree.

## Fail loud when required material is missing

Module options (`modules/options.nix`, `modules/secrets.nix`):

| Option | Role |
|--------|------|
| `surmount.secrets.requireDeployMaterial` | When true, activation fails if required paths are missing |
| `surmount.secrets.requiredHostPaths` | List of `{ path; kind; }` (`kind` = `file` or `directory`) |
| `surmount.secrets.deployMaterialDir` | Ephemeral conventional root (`/run/surmount-secrets`) |
| `surmount.secrets.durableMaterialDir` | Durable Domain B root (`/var/lib/surmount/secrets`) |

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
2. Prefer domain A custody: store material in GNOME Secret Service (schema in
   [docs/SECRETS.md](../docs/SECRETS.md)) or a private staging tree **outside**
   this repo. Optional: keep age admin identity offline (USB / paper), never git.
3. Create an age identity (workstation; **private; never commit**):

   ```bash
   mkdir -p ~/.config/sops/age
   age-keygen -o ~/.config/sops/age/keys.txt
   ```

4. Get the server host age key from its SSH host key (after first boot):

   ```bash
   ssh-to-age < /etc/ssh/ssh_host_ed25519_key.pub
   ```

5. Keep any `.sops.yaml` creation rules and encrypted payloads on the
   **operator machine or host**, not in the public repo. If a root `.sops.yaml`
   ever appears in-tree, it must contain **only** non-secret recipients/rules
   with no secret material; prefer keeping the whole sops workflow off-tree
   until a private-only ops path is designed.

6. Create secret files **outside git**, then **install onto the host** with
   `nix run .#secrets-install-host` (staging or secret-tool) or manual scp /
   console paste. Never write into public `hosts/` or this `secrets/` tree.

7. On the host, ensure decrypt key is available when using sops:
   - `/var/lib/sops-nix/key.txt`, or
   - SSH host key path listed in `modules/secrets.nix`.

8. Point `modules/secrets.nix` / host config at the **host-local** secrets
   file path and declare `sops.secrets.*` when using sops decrypt.

9. After B paths exist, consider enabling `requireDeployMaterial` on the
   **host** config (S6; operator-gated; not sample default-on).

## Example secret keys (names only)

| Key | Purpose |
|-----|---------|
| `mail/admin_password` | Stalwart admin |
| `mail/accounts/admin` | Account password |
| `backups/restic_password` | restic repo |
| `vaultwarden/admin_token` | Vaultwarden admin EnvironmentFile material (when enabled) |

Wire paths into `services.stalwart.credentials`,
`surmount.backups.passwordFile`, and Vaultwarden when enabled:

```text
# Example only (never real tokens in git):
# Host file: /run/surmount-secrets/vaultwarden/admin.env
# Contents: ADMIN_TOKEN=...   (KEY=value; mode 0600)
# Nix: surmount.vaultwarden.adminTokenEnvFile = "/run/surmount-secrets/vaultwarden/admin.env";
# Install: nix run .#secrets-install-host (domain A/B bridge). Sample host stays enable=false.
```

## Offline recovery (do not keep only on the mail disk; never in git)

- Workstation age identity backup
- LUKS recovery / passphrase (if FDE enabled; see SECURITY.md)
- restic password
- Vaultwarden emergency / admin materials (when host-enabled)

## Related

- [docs/hygiene.md](../docs/hygiene.md) - NEVER secrets in git
- [docs/SECRETS.md](../docs/SECRETS.md) - three domains, schema, runbook, bridge
- [docs/SECURITY.md](../docs/SECURITY.md) - threat model, FDE, Nostr auth notes
- [docs/OPS.md](../docs/OPS.md) - bring-up order + secrets install
- `modules/secrets.nix` - NixOS wiring
- `nix run .#secrets-install-host` - domain A -> B install bridge
