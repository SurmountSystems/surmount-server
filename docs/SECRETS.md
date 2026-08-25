# Secrets architecture (two buckets + optional disk encryption)

How Surmount handles secrets without mashing deploy crypto, human password
managers, and disk encryption into one vague bucket. Custody also spans
**operator workstation** vs **host at activation** (three domains below).

**Last updated:** 2026-08-25 (operator bins are `nix run .#...`; hermetic
crate tests in `checks.*.ci`. No leftover product `script/*.sh` drivers.)
**Prior:** 2026-08-18 (tls-key install mode is **0640**
`surmount-ui:surmount-tls`; laptop renew stages tls-cert/tls-key.
Prior 2026-08-12: session-secret EnvironmentFile shape +
nostr-allowlist one-shot for public edge B4; dual-pin OPS/SECURITY)
**Operator direction:** [operator-direction.md](operator-direction.md)
**Companions:** [hygiene.md](hygiene.md) (top rule), [SECURITY.md](SECURITY.md),
[DATASTORES.md](DATASTORES.md), [glossary.md](glossary.md),
[deploy-host-local.md](deploy-host-local.md) (private overlay + deploy driver),
[OPS.md](OPS.md) (cutover gates + free-443 + **Fix public dashboard** +
**Stalwart recovery unlock** + **Bootstrap Stalwart API token** +
**Add Stalwart admin token**),
[DNS.md](DNS.md) (Day-1 vs zone API),
[`secrets/README.md`](../secrets/README.md), `modules/secrets.nix`.
**Intake CLI (Domain A):** [`crates/surmount-secrets-prompt`](../crates/surmount-secrets-prompt)
(`just secrets-prompt` / `nix run .#surmount-secrets-prompt`). When the
admin password is **unknown**, prefer full unlock with
`just fix-public-dashboard` /
`nix run .#surmount-fix-public-dashboard`
(urandom recovery pin + mint + free-443 dry-run). Recovery-only:
`just stalwart-recovery-unlock` /
`nix run .#stalwart-recovery-unlock`
(kind `stalwart-recovery-admin`). When you have the first admin password,
prefer minting an API key with
`just bootstrap-stalwart-token` /
`nix run .#bootstrap-stalwart-api-token`
(WebUI optional). When you already hold an engine-accepted key string:
`just add-stalwart-token` /
`nix run .#add-stalwart-token`. Pipe,
`--token-file`, or interactive no-echo paste into private staging; existing
Domain A + `--target` installs without re-prompt; optional `secret-tool`.
Prefer this over chat or shell history.
**Bridge:** `nix run .#secrets-install-host` / `just secrets-install-host`
(crate tests in `checks.*.ci`).
**Deploy opt-in glue (S4):** `nix run .#surmount-deploy-host -- --install-secrets`
(crate tests in `checks.*.ci`).
**Host cutover (A0 + V5 + ladder):** `nix run .#surmount-host-cutover`
(`--step material|prep|install|free-443|dns|deploy|prove|le-prod|all`; inventory
+ install plan + ACME parents + host-profile + remote free-443 + private VW
enable fragments; crate tests in `checks.*.ci`). Laptop ladder one-
pager: [OPS.md](OPS.md) section *Laptop-driven cutover ladder*.
**Free Stalwart :443 (V1):** `nix run .#free-stalwart-public-443`
(crate tests in `checks.*.ci`).

**Language:** prefer **encrypted deploy secrets** or **secrets available at
NixOS activation**. Avoid vague "seal" jargon.

**Status (S0-S4 + A0 + V0/V1 + V4 + S8 durable Domain B offline shipped):**
three-domain contract (Domain C = human custody only; cutover automation =
A→B + host-local + DNS APIs), **durable Domain B** default
`/var/lib/surmount/secrets` for session/token/namecheap/VW admin (survives
reboot; laptop remains custody SoT), ephemeral `/run/surmount-secrets` still
allowlisted (PEMs/ACME account), attribute schema including **`namecheap-api`**,
material inventory, operator runbook (**V4** human vault + optional BW export),
hermetic install bridge, **opt-in** deploy-host glue, **A0 host cutover pack**,
and **free-443 driver** are in tree. Default deploy still does **not**
auto-install secrets. **Not** claimed: product libsecret Rust crate, live
keyring as CI green, live host install as CI green, live free-443 on a VPS, S5
Rust helper, S6 production `requireDeployMaterial` with real values on a box, or
live S7b unit on a VPS. **S7a** module offline is done (**do not rebuild**).
**S7b** enable path is **scripted** (kind `vaultwarden-admin` + cutover
`--with-vaultwarden`); operator still supplies the token **value** once (or
`--generate-material`) and runs live switch. Optional export helper is **not**
activation and **not** required for boot. Sample public host stays
`enable=false`.

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

**Pre-commit / CI gate:** `surmount-private-data` scans staged (and in
CI, tracked) files for private-data **pattern classes**. Patterns
only; no real secrets live in the tool. Project hook:
`script/git-hooks/pre-commit`. See [hygiene.md](hygiene.md) subsection
*Pre-commit private-data scan*.

---

## 1. Operator workstation vs host (three domains)

**Status:** operator direction **2026-08-09** / plan-approved living contract
(S0-S4 offline shipped: install bridge + opt-in `deploy-host --install-secrets`,
default off). Not full product acceptance of a libsecret Rust crate or live CI
against a keyring. **Truth today:** GNOME Secret Service is the **domain A
candidate**; operators use `secret-tool` / Seahorse by hand. Tree ships a
**install bridge** (`nix run .#secrets-install-host`) that reads a
**staging directory** (primary; must stay **outside** the public git work
tree) or optional `secret-tool` lookup, plus opt-in deploy glue. There is
**no** Surmount libsecret Rust crate and **no** GNOME module.

Secrets must never live in the project git tree. Prefer **OS-level secret
stores** for operator-side custody. Host needs activation material as files.

| Domain | Where | Job | Not its job |
|--------|-------|-----|-------------|
| **A. Operator workstation** | Laptop / admin machine OS secret store. **Candidate:** GNOME Secret Service (libsecret / gnome-keyring). Staging dir export is also valid for headless. | Custody of age admin keys, deploy-material **values + labels**, operator SSH material. Operator-side SoT for "what do I hold offline." | **Not** the VPS boot / NixOS activation SoT. Does not start units on the mail box by itself. |
| **B. Host deploy secrets** | Files / systemd credentials / sops-nix (or **Q-DEP-1** alternative) **on the host at activation**. **Default durable root:** `/var/lib/surmount/secrets/...` (survives reboot). **Optional ephemeral:** `/run/surmount-secrets/...` (wiped on reboot). | Material services need before/during `nixos-rebuild` / unit start. Product today consumes **env + file paths only**. Free-443 CLI, ACME DNS hook, and units read **B only**. | Human day-to-day password UX; git storage; laptop keyring as live runtime on the VPS; Vaultwarden at boot. |
| **C. Human vault** | **Vaultwarden** (module offline shipped; host enable residual) | Day-to-day passwords, TOTP, notes, mail credential inventory UX for humans. **Optional copies** of deploy material for human memory and reinstall recovery after S7b. | Feeding `nixos-rebuild` at activation; encrypting mail RocksDB; Bitwarden Secrets Manager API (VW does not implement SM); **never** cutover automation SoT. |

### Laptop custody vs host Domain B (durable + optional ephemeral) (2026-08-11 S8)

| Where | What lives there | What does **not** |
|-------|------------------|-------------------|
| **Laptop (Domain A)** | Long-term secrets: Namecheap API key, session secret, mail admin token notes, Secret Service / private staging. **Custody SoT.** | Treating the VPS as the only vault; skipping reinstall when Domain A is lost |
| **VPS durable Domain B** (`/var/lib/surmount/secrets/...`) | Activation copies that **survive reboot** (H-PEM default for new profiles): TLS PEMs + ACME account JSON, session secret, Stalwart API token, Namecheap env, Vaultwarden admin env (files 0600; material root 0755; leaf parents 0750 surmount-ui; product state prefix `/var/lib/surmount` 0755 so UI can traverse). Module option `surmount.secrets.durableMaterialDir`. | CA PEMs invented in git; Vaultwarden pull at boot |
| **VPS ephemeral Domain B** (`/run/surmount-secrets/...`) | Optional short-lived copies only (tmpfs; wiped on reboot; re-issue PEMs OK). Module option `surmount.secrets.deployMaterialDir`. Still allowlisted when the operator overrides profile paths. | The only path for session/token/namecheap after S8; not the recommended PEM default after H-PEM |
| **VPS Nix store** | DNS-01 helper **program** (`pkgs.acme-dns-hook-namecheap` / store path for `dnsHookPath`). That is **code**, not a secret. | API credentials inside the package |

**After reboot:** durable Domain B files under `/var/lib/surmount/secrets` remain.
Ephemeral `/run/surmount-secrets` is wiped. Re-run **install** only when you
need to refresh values or still use ephemeral paths. Prefer durable defaults
from `secrets-prompt` / cutover generate / inventory.

**Optional ClientIp rewrite at install only:** `secrets-install-host --client-ip IP`
rewrites only the `ClientIp=` line in the **materialized** namecheap.env for
VPS-side API calls. Never invents IP when the flag is omitted. Never logs the
secret body. Prefer this only when the VPS public IP must be the Namecheap
API caller whitelist entry.

**Require Namecheap for ACME path when asked:** `--require-namecheap` (or
cutover `--require-namecheap`) fails closed if kind `namecheap-api` is missing.
Never invents Namecheap credentials.

Optional later residual (not shipped): zero credentials on the VPS even briefly
(laptop-driven every DNS-01 challenge). Parked as H4 if install runtime copy
hurts in practice.

**Explicit bridge rule:** laptop Secret Service (**A**) does **not** replace
host-at-activation files (**B**). The workstation install bridge materializes
**B** paths over SSH (or a local `--dest-root` tree for tests). It never
writes into public `hosts/` or `secrets/` in the git tree.

**Cutover automation rule (V0 pin):** automate ACME-live cutover via
**A → B install + host-local public config + DNS APIs**. Do **not** automate
**C → activation**. Vaultwarden is human custody (Domain C). There is no
in-tree VW → Domain B pull at boot. Optional export C → staging for reinstall
is operator-gated and residual (not activation).

**Operator one-pager (S9):** human path is **Vaultwarden (C) holds a copy**
-> **export or `secrets-prompt` into Domain A** -> **install Domain B** ->
**free-443 / ACME** (B only). After Domain A is filled, the automatic ladder
is install -> free-443 -> ... (not management-ui or free-443 calling VW at boot).
Living ops prose: [OPS.md](OPS.md) section *Vaultwarden human path*. Depth
for what-lives-where and export helper: sections 5.1 and 5.2 below.

**Open (do not invent answers):** long-term deploy-secrets *tool* stays
**Q-DEP-1** (sops-nix vs alternative). True Secrets Manager API need remains
**Q-SEC-SM-***. Domain C Vaultwarden module timing stays residual / separate.
Logical host id naming and Arti HS bundle packaging details remain operator
choice (plan open questions).

LUKS2 disk encryption stays a **separate** concern (initrd unlock; never git).
See section on disk below and
[research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md).

### 1.1 Material inventory (comprehensive map)

| Material | Domain A (label / custody) | Domain B (host path convention) | Product consumer today |
|----------|----------------------------|----------------------------------|------------------------|
| TLS cert + key | yes (static PEM path) **or** none (ACME path) | **durable default** `/var/lib/surmount/secrets/tls/{cert,key}.pem` (key **0640** `surmount-ui:surmount-tls`, not world-readable; shared with Stalwart); optional ephemeral `/run/surmount-secrets/tls/...` | `managementUi.tlsCertPath` / `tlsKeyPath` (static read or ACME write targets) |
| ACME account JSON | optional backup only | **durable default** `/var/lib/surmount/secrets/acme/account.json`; optional `/run/...` | `managementUi.acme.accountCredentialsPath` (binary creates/restores; never git) |
| ACME parent dirs only | n/a (empty dirs) | durable `.../secrets/tls/` + `.../secrets/acme/` (default); optional `/run/surmount-secrets/{tls,acme}/` | L2: writable by `surmount-ui` so ACME can issue without pre-placed CA PEMs |
| Session HMAC secret | yes (`kind=session-secret`) | **durable** `/var/lib/surmount/secrets/ui/session-secret` (**EnvironmentFile** body: `SURMOUNT_SESSION_SECRET=...`) | `sessionSecretPath` -> unit EnvironmentFile -> `SURMOUNT_SESSION_SECRET` |
| Nostr allowlist (npub/hex only; **not** nsec) | yes for public edge B4 (`kind=nostr-allowlist`) | **durable** `/var/lib/surmount/secrets/ui/nostr-allowlist` (or env string); **0600** `surmount-ui` (UI reads the file) | `nostrAllowlistFile` / `nostrAllowlist` -> `SURMOUNT_NOSTR_ALLOWLIST_FILE` / env |
| Stalwart management token | yes (`kind=stalwart-token`) | **durable** `/var/lib/surmount/secrets/ui/stalwart-api-token` | `stalwartTokenPath`; free-443 driver; UI directory when enabled |
| Stalwart recovery admin pin | yes when unlocking (`kind=stalwart-recovery-admin`) | **durable** `/var/lib/surmount/secrets/stalwart/recovery.env` | `STALWART_RECOVERY_ADMIN=user:pass` EnvironmentFile; load via module `services.stalwart.recoveryAdminEnvFile` (path only) or private drop-in (password never in Nix/drop-in body; never unit `Environment=`). Prefer `--strip` after permanent API key (host drop-in + Domain B only; Domain A and module option remain until operator clears them) |
| Vaultwarden admin token | yes when S7b (`kind=vaultwarden-admin`) | **durable** `/var/lib/surmount/secrets/vaultwarden/admin.env` | VW EnvironmentFile |
| Namecheap API env (DNS-01 + zone) | yes (`kind=namecheap-api`; or C copy after S7b) | **durable** `/var/lib/surmount/secrets/acme/namecheap.env` | ACME hook + optional `nix run .#surmount-dns-zone` (file only after product `env_clear`) |
| SHC customer user-api (rDNS / operate) | yes (`kind=shc-api`) | **durable** `/var/lib/surmount/secrets/rdns/shc.env` | optional `nix run .#surmount-shc` / `just rdns-shc` (PTR via SHC API; console still valid) |
| Arti HS state / identity | yes (install package) | `onionServiceStateDir` | arti module |
| restic password | yes | `backups.passwordFile` (often sops path) | backups module |
| Age host identity | yes (bootstrap) | `/var/lib/sops-nix/key.txt` and/or host SSH key | sops-nix |
| Age admin identity | yes (**workstation only**) | **never on VPS as admin**; install bridge **refuses** `kind=age-admin` unless `--allow-age-admin-install` | encrypt / recovery |
| DiskStation AFP LAN password | yes (`kind=synology-afp`, host `DS1513` or `DS3018xs`; one item per NAS; host id is the label, not the AFP IPv4). Intake also writes GNOME NetworkPassword (`protocol=afp`, `server` = host id and `<id>.local`, never an IPv4) so Nautilus / raw gio remember | **never on VPS**; install bridge **always refuses** this kind | laptop `just diskstation-afp-mount -- --host DS1513` (optional `--afp-host` IPv4 / `--uri` for reach only); gio AFP |
| SSH operator key / authorized_keys | SSH agent + host-local | host-local overlay (not public `hosts/`) | sshd / deploy lockout check |
| LUKS unlock | separate | initrd (not Secret Service as sole unlock without explicit design) | disk |
| Mail user passwords | domain C (human track/share) | Stalwart engine (hashes) | not deploy bridge v1 |
| Edge hostnames / LE email | optional note only | **host-local public config** (not Domain B secret) | `managementUi.acme.domains` / email; render via host profile (section 1.0.2) |
| Human recovery notes / TOTP / app passwords | domain C primary | n/a at activation | Vaultwarden after S7b; never feeds rebuild |

Placeholders only in docs. Never real PEMs, IPs, tokens, or age keys in git.

### 1.0.2 Private host profile vs Secret Service vs Vaultwarden (V2)

Non-secret cutover config (edge hostnames, LE contact email, ACME directory
URL) does **not** belong in Domain A secret stores or Domain C as the
activation path. Use a **private host profile** outside public git, then
render into private host-local.

| Store | Holds | Does not hold |
|-------|-------|----------------|
| **Host profile** (e.g. `~/.local/share/surmount/host-profile.toml`) | `acme_domains` (certificate hostnames: services + mail + apex + www + mta-sts), `acme_email`, `acme_directory` (staging default; production only with explicit flag or profile field), optional `host_id`, path overrides | Namecheap ApiKey, PEMs, session/Stalwart/VW tokens |
| **Secret Service / staging (Domain A)** | Secret **values** + labels (`namecheap-api`, `stalwart-token`, …) | Edge FQDNs as the SoT for Nix ACME (use profile / host-local) |
| **Domain B files** | Installed secrets + ACME parents; hook reads `namecheap.env` | Git-tracked host config |
| **Vaultwarden (Domain C)** | Human notes / optional copies after S7b | Activation; profile fields as boot SoT |
| **Private host-local** | Rendered `host-local-acme.nix`, keys, hardware | Public git tree |

```bash
# Public sample only (example.invalid): script/testdata/host-profile/sample-host-profile.toml
# Copy outside the tree, fill real domains/email, never commit secrets.

just render-host-profile-acme -- \
  --profile ~/.local/share/surmount/host-profile.toml \
  --out /path/to/private/host-local
# Staging default. Production LE (explicit only; H6):
#   ... --directory production --force
# Certificate hostnames: sample lists services + mail + apex + www (H5). Live leaf also covers mta-sts.

# Flake known-name auto-import: host-local-acme.nix (when no default.nix).
# Hermetic: just test-render-host-profile-acme
```

Companions: [OPS.md](OPS.md) ACME enablement, [deploy-host-local.md](deploy-host-local.md),
[DNS.md](DNS.md) zone tool, [EDGE_AND_TLS.md](EDGE_AND_TLS.md) certificate hostnames + LE flip.

### 1.0 Five ACME-live cutover gates (class table)

Honest classes for what automation can own. Pre-placed CA PEMs are **not**
required (ACME issues into Domain B).

| Gate | Class | Automation target |
|------|--------|-------------------|
| 1. Stalwart admin token for free `:443` | **Secret** + **engine registration** | Domain A custody → Domain B file (`stalwart-token`) → `nix run .#free-stalwart-public-443`. **Must** be a value Stalwart **already accepts**. Installing or randomly generating the file is **not** engine registration. Live free-443 **401** = wrong/unknown admin credential. When password is **unknown**: `just fix-public-dashboard` (urandom recovery pin kind `stalwart-recovery-admin` + mint + free-443 dry-run). When admin password is known: `just bootstrap-stalwart-token` (CLI Basic auth + `create apikey`; WebUI optional). When you already hold the key string: `just add-stalwart-token`. Dual-pin: [OPS.md](OPS.md) *Fix public dashboard* + *Bootstrap Stalwart API token* + free-443 callout |
| 2. Edge hostname(s) for `acme.domains` | **Public config** | Private host-local / non-secret config map (not VW activation) |
| 3. LE contact email | **Public-ish config** (PII-ish, not a high-value secret) | Same as hostnames: host-local / operator config |
| 4. Namecheap API `namecheap.env` | **Secret** | Domain A → Domain B install (`kind=namecheap-api`); hook reads B only |
| 5. A/AAAA/TXT at registrar | **DNS ops** (not a secret) | Day-1 manual registrar UI is valid; optional `nix run .#surmount-dns-zone` for A/AAAA/TXT (SPF/DKIM/DMARC; dry-run default; same Domain B credentials). DKIM **private** keys (dual-sign) are Domain B PEMs under `/var/lib/surmount/secrets/mail/dkim/stalwart-ed25519.pem` and `stalwart-rsa4096.pem` (not git; mode 0600, `stalwart-mail`); public TXT (`k=ed25519` at `stalwart._domainkey`, `k=rsa` at `stalwart-rsa._domainkey`) is not a secret. RSA floor 4096. Neither key is post-quantum (see [DNS.md](DNS.md) hardness note) |

### 1.0.1 Bootstrap chicken-egg order

```text
first secrets (staging or Secret Service or generate-material)
    -> install Domain B (session, stalwart-token file, optional namecheap-api, optional VW admin)
    -> free Stalwart :443 (needs token known to engine; free-443 driver)
    -> ACME + DNS hook (needs namecheap.env on B + hostnames in host-local)
    -> public HTTPS
    -> enable Vaultwarden (S7b) with admin on B
    -> humans use VW; optional export copies back to staging for reinstall
```

You cannot require **live Vaultwarden** before the first Domain B material
exists. You **can** use VW **after** S7b as the preferred human place to keep
Namecheap keys, Stalwart admin notes, and reinstall recovery, then re-export to
staging when reinstalling a host.

### 1.2 Secret Service attribute schema

Fixed attributes so `secret-tool`, Seahorse, and the install bridge stay
scriptable. Application attribute / label family: **`surmount`**.

| Attribute | Required | Meaning |
|-----------|----------|---------|
| `surmount.kind` | yes | Enum: `tls-cert`, `tls-key`, `session-secret`, `stalwart-token`, `restic-password`, `age-admin`, `age-host`, `arti-hs-bundle`, `nostr-allowlist`, `vaultwarden-admin`, `namecheap-api`, `shc-api`, `stalwart-recovery-admin`, `synology-afp`, `other` |
| `surmount.host` | yes (bridge enforces) | Logical host id (operator-chosen short name). **Not** a public IP committed to git. Must match `--host-id`. For `synology-afp` this is **DS1513** or **DS3018xs** (the Secret Service label, not the AFP IPv4, not a generic `diskstation`). |
| `surmount.path` | yes (for install) | Intended **host absolute path** after install (e.g. durable `/var/lib/surmount/secrets/tls/cert.pem`; optional `/run/...`). No `.` / `..` segments. **Omitted** for laptop-only `synology-afp`. |
| `surmount.version` or `surmount.updated` | optional | Rotate tracking |

**Secret value:** raw file bytes or single-line secret (kind-specific).

**Conventional Domain B paths (Secret Service → install map):**

| `surmount.kind` | Conventional `surmount.path` | Notes |
|-----------------|------------------------------|--------|
| `session-secret` | `/var/lib/surmount/secrets/ui/session-secret` (durable default; `/run/...` still allowlisted) | **EnvironmentFile** for management-ui: one line `SURMOUNT_SESSION_SECRET=<hex>` (mode 0600). `secrets-prompt --generate` emits raw 64-hex; **wrap** as KEY=value before install when using `sessionSecretPath`. Bare hex alone is **not** a valid systemd EnvironmentFile assignment |
| `nostr-allowlist` | `/var/lib/surmount/secrets/ui/nostr-allowlist` (durable preferred; `/run/...` allowlisted) | Operator **npub** and/or hex pubkeys only (comma/space/newline; `#` comments OK). **Never nsec.** Install as **0600** `surmount-ui:surmount-ui` (`kind_needs_ui_owner`; the UI process reads the file, it is not an EnvironmentFile). Empty file + mode=nostr = nobody authenticates (fail-closed gate still blocks console). Not in secrets-prompt kind enum; stage the file body and install with `--require-kind nostr-allowlist`, or place the host file by hand |
| `stalwart-token` | `/var/lib/surmount/secrets/ui/stalwart-api-token` (durable default; `/run/...` still allowlisted) | Raw token line; free-443 + UI directory. Value must be one **Stalwart already accepts**; generate/file alone ≠ engine registration (see §1.0 gate 1 + bootstrap honesty) |
| `stalwart-recovery-admin` | `/var/lib/surmount/secrets/stalwart/recovery.env` (durable default) | `STALWART_RECOVERY_ADMIN=admin:<password>` EnvironmentFile; agent unlock via `just stalwart-recovery-unlock` / `just fix-public-dashboard`. Unit load preference: module `recoveryAdminEnvFile` path only, else durable /etc drop-in, else /run. Hygiene: `--strip` after mint. Not free-443 auth by itself (mint API key after pin is live). |
| `namecheap-api` | `/var/lib/surmount/secrets/acme/namecheap.env` (durable default; `/run/...` still allowlisted) | KEY=value env for DNS-01 hook; mode **0600** required (hooks refuse group/world bits). Co-located under ACME account parent today (unit `ReadWritePaths` includes that parent); sibling leaf residual if write blast radius needs shrink (H4). |
| `shc-api` | `/var/lib/surmount/secrets/rdns/shc.env` (durable default) | KEY=value: `ApiKey`, `ApiBase`, optional `ServiceId`; rDNS tool |
| `vaultwarden-admin` | `/var/lib/surmount/secrets/vaultwarden/admin.env` (durable default; `/run/...` still allowlisted) | `ADMIN_TOKEN=...` EnvironmentFile; S7b |
| `tls-cert` / `tls-key` | `/var/lib/surmount/secrets/tls/{cert,key}.pem` (durable default; `/run/...` still allowlisted) | Static PEM path or ACME write targets; key mode **0640** `surmount-ui:surmount-tls` (do not chmod 0600). Laptop renew: `just laptop-renew-cert -- --live --directory production` stages Domain A then kind-filters these two kinds. |
| `restic-password` | operator-chosen under material roots | backups module |
| `synology-afp` | **none** (laptop-only; no Domain B path) | DiskStation AFP LAN password. One item per NAS. Labels `surmount synology-afp DS1513` and `surmount synology-afp DS3018xs`. Lookup: `secret-tool lookup surmount.kind synology-afp surmount.host DS1513` (and `DS3018xs`). Intake also stores GNOME `org.gnome.keyring.NetworkPassword` (`protocol=afp`, `user=hunter`, `server=DS1513` or `DS3018xs`, plus `<id>.local`) so Nautilus and raw `gio mount` remember (CLI `gio` does not set the save flag by itself). `--afp-host`, `--uri`, and laptop-private hint `~/.local/share/surmount/diskstation-afp-hosts` (never git) may still be IPv4 for the **mount URI only**. That IPv4 is never NetworkPassword `server` (LAN addresses can change) and is not the `surmount.host` label. `--generate` refused. `secrets-install-host` **always refuses**. Intake: `just secrets-prompt -- synology-afp --host DS1513 --secret-service --no-staging` (optional `--afp-host`). After a successful `diskstation-afp-mount`, missing NetworkPassword is filled from the same lookup (stdin, never argv). After every `secret-tool` / `secrets-prompt` store of any kind, paste buffers are wiped (empty stdin to `xclip` clipboard and PRIMARY, `pbcopy`, and `wl-copy --clear` when present). Never pipe the secret into the clipboard. |

**UI:** Seahorse / `secret-tool` for operator day-to-day. Prefer
`just secrets-prompt` (no-echo intake into staging; optional subprocess
`secret-tool`) for cutover kinds. In-process libsecret Rust (**S5**) remains
**parked**, not shipped.

### 1.3 Bridge rules (S3 offline)

| Rule | Detail |
|------|--------|
| Domain A does not activate B | Keyring / staging is custody; host units need files on the box |
| Primary mode: staging dir | Operator fills a private staging tree **outside** the public git work tree; `nix run .#secrets-install-host -- --from-staging` installs (deploy `--install-secrets` refuses in-tree staging) |
| Staging secret files | Must be **regular files** (not symlinks). Bridge materializes bytes only; refuses link payloads |
| Optional: secret-tool | `--from-secret-service` when `secret-tool` is on PATH and session unlocked. Default SS kind list is **https-lean** (tls-cert, tls-key, session-secret, stalwart-token, restic-password). `vaultwarden-admin` and `namecheap-api` are allowlisted but **not** in the SS default; pass `--kind` for those (S7b / ACME DNS-01) |
| Destinations | SSH `--target` / `SURMOUNT_DEPLOY_TARGET` (or `SURMOUNT_SECRETS_TARGET`), or local `--dest-root` for hermetic / fake remote |
| dest-root confinement | Resolved final path must stay under `--dest-root` (root resolved once at start; single-operator trust); `surmount.path` must not contain `.` / `..` segments |
| Host path allowlist | `surmount.path` must be under durable `/var/lib/surmount/secrets`, ephemeral `/run/surmount-secrets`, or `/var/lib/sops-nix` (deploy material roots) |
| Outside public git tree | Local installs refuse writes under the public repo work tree (not only `hosts/` / `secrets/`) |
| Modes on install | File mode **0600** regular file; staging temp **0700**; under durable/ephemeral material: material root **0755**, leaf parents (`tls/`, `acme/`, `ui/`, ...) **0750**; product state prefix `/var/lib/surmount` **0755** when created under dest-root (H2b UI traverse). Never chmod system parents (`/run`, `/var`, `/etc`); wipe internal temp after install |
| ACME parents mode | `--ensure-acme-parents`: create **dirs only**. **Default durable** `/var/lib/surmount/secrets/{tls,acme}` (material **0755**, leaves **0750** surmount-ui; state prefix **0755**). Ephemeral `/run/...` via `--tls-dir` / `--acme-dir`. **No PEMs, no account.json payload.** No staging / host-id. Host-cutover `--acme-path` invokes this when `--target` or `--dest-root` is set. Durable boot: management-ui **tmpfiles** for durable `tls/` + `acme/` + configured ACME parents (no secret contents). |
| Final path postcondition | Refuse if destination is a directory or symlink (local before write; remote before write + after `mv`). No secret debris on refuse |
| Remote install | One SSH channel (stdin + `umask 077` + temp then `mv`); no scp world-readable window |
| Refuse public inject | Never write under public product `hosts/` or `secrets/` (path segments, missing parents, symlink targets; same spirit as `nix run .#surmount-deploy-host`) |
| age-admin | Refused by default (workstation-only). Requires `--allow-age-admin-install` dangerous override |
| synology-afp | **Always refused** (laptop-only DiskStation AFP LAN password). No override. Never copy the NAS password to the mail host |
| surmount.host required | Staging items must set `surmount.host` matching `--host-id` (not required for `--ensure-acme-parents`) |
| Never log values | Labels, kinds, paths, exit codes only; control characters rejected in kind/path/host-id |
| Reject option-shaped targets | SSH targets starting with `-` fail loud |
| Missing required kind | `--require-kind` missing -> non-zero; no partial success claim (checked before any install) |
| Multi-item I/O failure | Not transactional: if item N fails mid-loop, earlier items may already be written; process exits non-zero and does not claim full success |
| CI | Hermetic crate tests in `checks.*.ci` only. **Never** live keyring or live host as green CI |

**ACME vs static PEMs (Domain B):** the ACME product path does **not** require
operator-placed CA cert/key PEMs. Create writable parents (bridge
`--ensure-acme-parents` and/or tmpfiles when `acme.enable`), then let the binary
issue into `tlsCertPath` / `tlsKeyPath` and persist account credentials under
`accountCredentialsPath`. Static PEMs remain first-class when ACME is off.

**sops-nix relationship (do not invent Q-DEP-1):** keep sops-nix as optional
host-side decrypt scaffold where it already helps (e.g. restic). Prefer
**direct install of owner-only files** for TLS PEMs, session secret, Stalwart
token, and Arti state when simpler. Secret Service holds operator-side material
and/or age admin keys used to encrypt host-local sops files **out of tree**.
Ciphertext still never lands in public git.

**Not chosen for v1:** query laptop keyring from VPS at boot; full Secrets
Manager API; dual agenix + sops; encrypted-in-git shortcuts.

### 1.4 Operator day-1 flow (after S3)

1. Unlock GNOME session (Secret Service available), or prepare a private
   staging directory offline.
2. Ensure items exist for this logical host (schema attributes; section 1.5
   runbook).
3. Install domain B material (pick one):
   - Standalone: `nix run .#secrets-install-host` (or `just secrets-install-host`)
     with target SSH **or** staging + later scp, plus logical `--host-id`
     (env only; no committed IPs).
   - **Opt-in S4:** `nix run .#surmount-deploy-host -- --install-secrets` with
     `--secrets-staging` (or `--secrets-from-secret-service`) and
     `--secrets-host-id` so the deploy driver invokes the bridge **before**
     rebuild. Default deploy without `--install-secrets` does **not** install
     secrets.
4. `nix run .#surmount-deploy-host` / `just deploy-host` (or rebuild) for public tree +
   host-local when not already covered by the S4 combined run.
5. Smoke: `just e2e-host` when public HTTPS up; optional hybrid TLS probe.
6. Enable `requireDeployMaterial` once paths are stable (**S6**, operator host;
   not sample default-on).

**S4** opt-in deploy-host glue is **shipped offline** (default still pure
sync+rebuild). **S5** Rust helper parked. **S7a** Vaultwarden NixOS module +
management console link **shipped offline** (sample host enable=false).
**S7b** host enable + real admin token is operator residual. **Q-SEC-SM-***
remains open (true SM API not invented).

### 1.5 Operator runbook (S1): secret-tool / Seahorse / staging

#### Prefer: interactive intake CLI (no-echo)

**Use this instead of pasting Namecheap ApiKey, tokens, or PEMs into chat,
agent prompts, or shell history.** The first-party binary
`surmount-secrets-prompt` prompts on a TTY without echo (same idea as
`ssh` / `sudo` password prompts), writes Domain A staging
(`kind/attributes` + `kind/secret` mode 0600), and can optionally call
`secret-tool store` when you pass `--secret-service`.

```bash
# Namecheap API env (ApiKey never echoed). Interactive mode explains each field
# in plain English. Prefer a full domain (example.com); SLD/TLD are derived.
# --host is Surmount inventory name (for example surmount-1), not Namecheap.
just secrets-prompt -- namecheap-api --host mail-lab
# or:
cargo run -p surmount-secrets-prompt -- namecheap-api --host mail-lab
# Batch: --domain example.com (or --sld / --tld), plus field flags + --secret-file

# SHC customer user-api key for PTR/rDNS (operate scope preferred; never invent)
just secrets-prompt -- shc-api --host surmount-1
# Then: just rdns-shc -- --list   # or --live after FCrDNS is green
# Optional Domain B: --require-kind shc-api -> /var/lib/surmount/secrets/rdns/shc.env

# Mint Stalwart API key from admin password (preferred when password known; WebUI optional)
just bootstrap-stalwart-token -- --password-file ~/.config/surmount/admin-pass \
  --target root@HOST

# Stalwart admin token when you already hold the key string (defaults host=surmount-1)
just add-stalwart-token
printf '%s\n' 'REAL_TOKEN' | just add-stalwart-token -- --host surmount-1 --target root@HOST
just add-stalwart-token -- --token-file ~/.config/surmount/stalwart-token \
  --host surmount-1 --target root@HOST
# Existing Domain A staging + --target: install only (no re-prompt)
just add-stalwart-token -- --host surmount-1 --target root@HOST

# Single-line tokens / session secret (lower-level secrets-prompt)
just secrets-prompt -- stalwart-token --host surmount-1
just secrets-prompt -- session-secret --host surmount-1 --generate
just secrets-prompt -- vaultwarden-admin --host surmount-1 --generate

# DiskStation AFP LAN password (laptop Secret Service only; never install to VPS)
# One item per NAS. Host ids: DS1513 (5-bay) and DS3018xs (6-bay).
# --host is the Secret Service label. DSM rename / mDNS are optional.
# Mount may pass --afp-host IPv4 or --uri; do not commit office LAN IPv4.
just secrets-prompt -- synology-afp --host DS1513 --secret-service --no-staging
just secrets-prompt -- synology-afp --host DS1513 --afp-host <IPv4-or-hostname> --secret-service --no-staging
just secrets-prompt -- synology-afp --host DS3018xs --secret-service --no-staging
# Optional: just diskstation-discover
# Then: just diskstation-afp-mount -- --host DS1513
#    or: just diskstation-afp-mount -- --host DS1513 --afp-host <IPv4-or-hostname>
# Lookup: secret-tool lookup surmount.kind synology-afp surmount.host DS1513
# Nautilus/gio remember via NetworkPassword server=DS1513 (or DS1513.local),
# never an IPv4. After store, paste buffers are wiped (empty stdin).
# One-time paste above is enough; do not put the password on argv.

# Custom staging root (default: $XDG_DATA_HOME/surmount/staging or
# ~/.local/share/surmount/staging). Keep outside the public git work tree.
just secrets-prompt -- namecheap-api --host mail-lab \
  --staging "$HOME/.local/share/surmount/staging"

# Optional Secret Service store (requires secret-tool on PATH + unlocked session)
just secrets-prompt -- session-secret --host mail-lab --generate --secret-service

# Then install Domain B (values never logged):
just secrets-install-host -- --from-staging "$HOME/.local/share/surmount/staging" \
  --host-id mail-lab --target root@YOUR_HOST --require-kind namecheap-api
just secrets-install-host -- --from-staging "$HOME/.local/share/surmount/staging" \
  --host-id surmount-1 --target root@YOUR_HOST --require-kind shc-api
```

Hermetic CI: `just test-secrets-prompt` / `cargo test -p surmount-secrets-prompt`
(batch mode + synthetic fixtures only; **never** live keyring as green).

S5 in-process libsecret Rust remains **parked**. This CLI uses staging as
primary and subprocess `secret-tool` only when requested.

##### Namecheap field cheat sheet (plain English)

| Field | What to enter |
|-------|----------------|
| **Logical host id** (`--host`) | Surmount inventory name (for example `surmount-1`). **Not** Namecheap ApiUser, domain, or IP. |
| **ApiUser** | Usually your Namecheap account username (login name for many personal accounts). |
| **UserName** | Optional Namecheap API UserName. Leave blank if same as ApiUser; set only for a sub-user. |
| **Domain / SLD + TLD** | Prefer full registered domain (`example.com`). SLD = second-level name (`example`); TLD = top-level (`com`, or `co.uk`). Tool accepts `--domain` or interactive domain prompt. |
| **ApiKey** | From Namecheap Profile -> Tools -> API Access. Never echoed. |
| **ClientIp** | Public IP of the **machine calling** the Namecheap API (laptop for local tools, VPS for host ACME). Not the domain; not Namecheap's IP. |

##### Laptop intake vs Namecheap `ClientIp`

**Where you run `secrets-prompt` is not the same thing as `ClientIp`.**

| Concept | What it means |
|---------|----------------|
| **Domain A intake** | Run `just secrets-prompt` / `surmount-secrets-prompt` on the **operator laptop** (home network / local IP). No-echo paste into private staging. That is **correct and preferred**. Typing the secret on the VPS is not required. |
| **Namecheap `ClientIp`** | The public IP **Namecheap sees on API HTTP calls**, not "where you typed the secret." It must match an IP you whitelist for Namecheap API access. |
| **ACME DNS-01 hook** | Runs on the **mail host** when management-ui issues certs. Live API calls come from the **VPS public IP**. Whitelist the **VPS** for live ACME. |
| **Laptop-side tools** | `dns-zone-namecheap` or secrets-prompt dry exploration from the laptop make API calls from the **laptop public IP**. Whitelist that laptop egress IP (or both IPs if Namecheap allows multiple). |

**Recommendation:**

1. Whitelist **both** laptop and VPS public IPs for Namecheap API access when practical.
2. Put the IP that matches the **machine that will run** the hook or zone tool into `ClientIp` for that env file.
3. When one `namecheap.env` is installed to Domain B for ACME, **`ClientIp` should be the VPS** (the hook's egress). Intake still happens on the laptop; install bridge copies the file to the host.

Do not invent multi-IP UI details beyond: whitelist the API caller IP(s). Never put real IPs or keys in this public tree. Companion: [OPS.md](OPS.md) Namecheap DNS-01 hook install.

#### Put an item with secret-tool

```bash
# Example labels only. Use real PEM/secret bytes from a private path, never
# from this public git tree. Do not paste real values into chat or docs.
# Prefer `just secrets-prompt` for cutover kinds when you can.
secret-tool store --label='surmount tls-cert mail-lab' \
  surmount.kind tls-cert \
  surmount.host mail-lab \
  surmount.path /run/surmount-secrets/tls/cert.pem
# secret-tool prompts for the secret on stdin (or pipe from a private file).
```

Seahorse: create a password/item in the login keyring; set the same
`surmount.*` attributes if the UI allows custom attributes (schema above).

#### Cutover-critical kinds (SS / staging runbook rows)

Do **not** paste real values into docs, chat, or git. Labels and paths only.

| Kind | Domain B path | Value shape | SS store sketch |
|------|---------------|-------------|-----------------|
| `session-secret` | `/var/lib/surmount/secrets/ui/session-secret` (durable; `/run/...` allowlisted) | **EnvironmentFile:** `SURMOUNT_SESSION_SECRET=<64 hex>` (wrap raw generate output) | `secret-tool store --label='surmount session-secret HOST' surmount.kind session-secret surmount.host HOST surmount.path /var/lib/surmount/secrets/ui/session-secret` |
| `nostr-allowlist` | `/var/lib/surmount/secrets/ui/nostr-allowlist` (durable; `/run/...` allowlisted) | npub/hex lines only; never nsec | `surmount.kind nostr-allowlist` + path above; payload is allowlist text |
| `stalwart-token` | `/var/lib/surmount/secrets/ui/stalwart-api-token` (durable; `/run/...` allowlisted) | single raw token line (no `TOKEN=` prefix) | same pattern with `surmount.kind stalwart-token` and path above |
| `namecheap-api` | `/var/lib/surmount/secrets/acme/namecheap.env` (durable; `/run/...` allowlisted) | KEY=value lines (`ApiUser`, `ApiKey`, `ClientIp`, `SLD`, `TLD`, ...) | `surmount.kind namecheap-api` + path above; payload is the env file body; host file mode **0600** |
| `shc-api` | `/var/lib/surmount/secrets/rdns/shc.env` | KEY=value (`ApiKey=shc_live_...`, `ApiBase=...`, optional `ServiceId=`) | `surmount.kind shc-api` + durable path; operate key from SHC portal; never generate |
| `vaultwarden-admin` | `/var/lib/surmount/secrets/vaultwarden/admin.env` (durable; `/run/...` allowlisted) | `ADMIN_TOKEN=<non-empty>` EnvironmentFile | `surmount.kind vaultwarden-admin` + path above; S7b / cutover `--with-vaultwarden` |

Staging layout for `namecheap-api` (private tree only):

```text
private-staging/namecheap-api/
  attributes    # surmount.kind=namecheap-api
                # surmount.host=<logical-host-id>
                # surmount.path=/var/lib/surmount/secrets/acme/namecheap.env
  secret        # KEY=value env body (mode 0600 regular file)
```

Staging layout for `shc-api` (private tree only):

```text
private-staging/shc-api/
  attributes    # surmount.kind=shc-api
                # surmount.host=<logical-host-id>
                # surmount.path=/var/lib/surmount/secrets/rdns/shc.env
  secret        # ApiKey=... ApiBase=... optional ServiceId= (mode 0600)
```

Install:

```bash
nix run .#secrets-install-host -- --from-staging /path/to/private-staging \
  --host-id mail-lab --target root@YOUR_HOST \
  --require-kind namecheap-api
# or secret-tool: --from-secret-service --kind namecheap-api ...

nix run .#secrets-install-host -- --from-staging /path/to/private-staging \
  --host-id surmount-1 --target root@YOUR_HOST \
  --require-kind shc-api
# Then PTR: just rdns-shc -- --list / --live (docs/DNS.md)
```

**Stalwart bootstrap honesty (S8 pin):**

- Installing the Domain B `stalwart-token` file is **not** registering that
  value inside the Stalwart engine.
- Random generate into staging (`--generate-material` / `secrets-prompt
  --generate` for this kind) is **not** engine registration either. A new
  random string is only a file until Stalwart is taught to accept it (or until
  you replace it with a credential the engine already has).
- Free-443 needs a credential **Stalwart already accepts**. Live HTTP **401**
  means wrong or unknown admin credential, not a free-443 script bug.
- **When the admin password is unknown:** prefer
  **`just fix-public-dashboard -- --target root@HOST`**
  (`nix run .#surmount-fix-public-dashboard`).
  That generates a high-entropy recovery pin (kind `stalwart-recovery-admin`),
  installs durable Domain B
  `/var/lib/surmount/secrets/stalwart/recovery.env` as
  `STALWART_RECOVERY_ADMIN=admin:<password>`, loads it via unit
  `EnvironmentFile` (module path) or a **private** systemd drop-in that only
  sets `EnvironmentFile=-...` (password never in the drop-in or Nix store),
  restarts `stalwart-mail`, probes Basic auth, mints an API key via bootstrap,
  and free-443 dry-run. Prefer hygiene strip after mint:
  `just stalwart-recovery-unlock -- --strip --target root@HOST` or compose
  `--strip-recovery` (compose still strips when free-443 is BLOCKED after mint).
  Strip is host-only (drop-in + Domain B); Domain A staging remains; clear
  `services.stalwart.recoveryAdminEnvFile` on rebuild if set so the unit no
  longer wires the path. To keep pin across reboot: set
  `services.stalwart.recoveryAdminEnvFile` to the Domain B path only. Lower-level:
  `just stalwart-recovery-unlock`. Upstream recovery admin:
  [Recovery mode](https://stalw.art/docs/configuration/recovery-mode/)
  (accessed: 2026-08-11). Dual-pin: [OPS.md](OPS.md) *Fix public dashboard*.
- **When you have the first admin password** (bootstrap log / recovery pin /
  known login): prefer **`just bootstrap-stalwart-token`**
  (`nix run .#bootstrap-stalwart-api-token`).
  It uses CLI Basic auth, **ensures Domain + permanent Admin Account** when the
  directory is empty (idempotent if already present; same password as intake;
  permanent Admin password via create `--stdin`, never on argv logs), then
  `create apikey` as `admin@domain` after ensure (so the key attaches to
  directory Admin, not recovery bare user; Inherit permissions = full Admin
  bootstrap privilege), captures the one-time `Secret:` line (never logged),
  stages kind `stalwart-token`, and can install Domain B.
  Compose order with recovery: **recovery -> Domain -> permanent Admin ->
  ApiKey -> free-443**. **WebUI is optional** once the password is known; this
  CLI path is preferred. Upstream:
  [Creating objects](https://stalw.art/docs/management/cli/create/) and
  [Account](https://stalw.art/docs/ref/object/account/) (accessed: 2026-08-11).
- When you already hold an engine-accepted API key string: prefer
  `just add-stalwart-token` (or
  `just secrets-prompt -- stalwart-token --host YOUR_LOGICAL_HOST`), then
  install Domain B.
- Free-443 driver fails closed on auth error until the engine knows the token.
  Recovery pin path invents a **recovery** password only (not a free-443 token);
  the mint step still registers a real API key with the engine. See [OPS.md](OPS.md)
  *Fix public dashboard* + *Bootstrap Stalwart API token* + free-443 callout.

#### List / check without printing values

```bash
# Search returns attributes/labels; treat output as sensitive metadata still.
secret-tool search surmount.host mail-lab
secret-tool search surmount.kind tls-cert surmount.host mail-lab
```

Do **not** run `secret-tool lookup` in a shared terminal log if the value
would be captured. Prefer install bridge staging or direct install.

#### Staging directory layout (headless / hermetic / export)

Private directory (outside git), one subdirectory per item:

```text
private-staging/
  tls-cert/
    attributes    # surmount.kind=tls-cert
                  # surmount.host=mail-lab
                  # surmount.path=/run/surmount-secrets/tls/cert.pem
    secret        # payload bytes (mode 0600)
  tls-key/
    attributes
    secret
```

Install (local fake root for dry practice, or SSH to the box):

```bash
# Hermetic / practice (no SSH):
nix run .#secrets-install-host -- --from-staging /path/to/private-staging \
  --host-id mail-lab --dest-root /tmp/fake-root \
  --require-kind tls-cert --require-kind tls-key

# Dry-run against planned host (no write):
nix run .#secrets-install-host -- --dry-run --from-staging /path/to/private-staging \
  --host-id mail-lab --target root@YOUR_HOST

# Real install over SSH (operator machine; never CI green):
nix run .#secrets-install-host -- --from-staging /path/to/private-staging \
  --host-id mail-lab --target root@YOUR_HOST \
  --require-kind tls-cert --require-kind tls-key

# Optional secret-tool mode (unlocked session):
nix run .#secrets-install-host -- --from-secret-service \
  --host-id mail-lab --target root@YOUR_HOST \
  --kind tls-cert --kind tls-key

# Opt-in S4: deploy driver invokes the bridge before rebuild (default off):
nix run .#surmount-deploy-host -- --dry-run --target root@YOUR_HOST \
  --host-local /path/to/private/host-local \
  --install-secrets --secrets-staging /path/to/private-staging \
  --secrets-host-id mail-lab \
  --secrets-require-kind tls-cert --secrets-require-kind tls-key
```

`just secrets-install-host -- ...` mirrors the install bridge.
`just deploy-host -- ...` mirrors the deploy driver (`[positional-arguments]`).

#### Recovery notes

- Backup of domain A (keyring export / offline age-encrypted blob) is
  **operator-owned**. **Never** put recovery material in git.
- Losing only host files is recoverable if domain A still has values and the
  install bridge can re-materialize B paths.
- Losing age admin identity without offline backup may make host ciphertext
  unrecoverable; keep offline copies off the mail disk and off git.
- Domain C (Vaultwarden) is not the activation recovery path.

#### Self-test (CI / developer)

```bash
nix run .#surmount-private-data -- --tree   # exit 0; no real secrets in tree
# Hermetic secrets-install contracts live in crate tests (`checks.*.ci`).
```

Never wire live `secret-tool` or live SSH install as a green CI gate.

---

## 2. Two buckets (do not mash)

| Bucket | Name | Tool (today / direction) | Job | Not the job of |
|--------|------|--------------------------|-----|----------------|
| **1** | **Deploy secrets** (machine / config) | **sops-nix** + age (scaffold; *need* required even if tool changes) | Decrypt service secrets **on the host** at activation into restricted paths so systemd can start services | Human day-to-day password UX; FDE; git storage |
| **2** | **Vaultwarden** (human / org vault) | **Vaultwarden** (module offline; host enable residual) | Operator passwords, TOTP, secure notes, shared team items, mail credential inventory UX | Feeding `nixos-rebuild`; encrypting mail RocksDB |

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

## 3. Why deploy secrets are not Vaultwarden (operator FAQ)

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

## 4. Bucket 1: deploy secrets on the host (sops-nix scaffold today)

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

### Host-local overlay (deploy)

Box-specific private material (SSH pubs, hardware-config, real addresses,
hostname, and pointers to deploy-secret paths) belongs in a **host-local**
directory on the machine, not in public `hosts/` or `secrets/`. Contract,
placeholder layout, and deploy driver behavior (including refuse-copy into
tracked paths): [deploy-host-local.md](deploy-host-local.md). Conventional
on-host path after first deploy: `/root/surmount-server/host-local/` (example
only; not a product requirement of that exact string).

TLS PEMs default to durable `/var/lib/surmount/secrets/...` (H-PEM); `/run`
aliases and Arti HS identity still use operator-chosen host paths. host-local
is the **Nix/SSH/network overlay**, not a second secrets store to commit.

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
| `vaultwarden/admin_token` | VW admin EnvironmentFile material (when VW enabled; domain B install) |
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

## 5. Bucket 2: Vaultwarden (human vault)

**Ops one-pager first:** [OPS.md](OPS.md) *Vaultwarden human path* (C copy -> A
staging via `secrets-prompt` or `secrets-export-bw-to-staging` -> B install ->
free-443 / ACME). **No boot pull from VW.** Tables below are depth, not a
second activation story.

### Status (S7a offline done; S7b driver-owned after token; V4/S9 runbook offline)

| Slice | Status |
|-------|--------|
| **S7a** NixOS module `modules/vaultwarden.nix` + options `surmount.vaultwarden.*` | **Shipped offline** (**do not rebuild**) |
| Sample host `hosts/mail-vps` enable | **Default false** (commented checklist only) |
| Management console wire-up (`SURMOUNT_VAULTWARDEN_URL`, system/overview chips, Open vault link, `GET /api/v1/system` fields) | **Shipped offline** |
| Pure module-eval contracts (default off; enable needs token path; ConditionPathExists) | **Shipped** |
| **S7b** install kind + cutover enable path | **A0 offline shipped:** kind `vaultwarden-admin` -> `/run/surmount-secrets/vaultwarden/admin.env`; `host-cutover --with-vaultwarden` inventory + private enable fragment + smoke recipes |
| **S7b** live unit on a real VPS | **Host residual** after operator supplies token value (or `--generate-material`) + live target (G8-G10) |
| **V4/S9** human-store runbook + optional export to staging | **Offline shipped** (this section + OPS one-pager + `nix run .#secrets-export-bw-to-staging`); never activation |
| **Q-SEC-SM-*** true Bitwarden Secrets Manager API | **Open** (do not invent; VW is PM only) |

### Intent

- Self-hosted Bitwarden-compatible **password manager** API for humans and teams.
- Private listen by default (loopback Rocket); **no public signup**.
- Admin token from **host EnvironmentFile** (domain B), never inline in public tree.
- **Mail credentials** live in Stalwart; Vaultwarden is how humans
  track/integrate those secrets (app passwords, recovery notes), not the
  engine's directory store.
- Does **not** feed `nixos-rebuild` activation for other services' secrets.
- **Hard rule:** Vaultwarden **never** feeds `nixos-rebuild` / activation.
  Domain C is human custody and optional reinstall comfort only. Domain B
  files still come from domain A (staging / Secret Service) via the install
  bridge. See section 5.1.

### 5.1 Operator runbook: what lives where (V4)

After first domain B material exists and (optionally) S7b is live, treat
Vaultwarden as the **human** place to remember and recover. Do **not** invert
the chicken-egg: first secrets still start in domain A or generate-material.

| Material | Domain C (Vaultwarden) | Domain A (Secret Service / staging) | Domain B only (host files) |
|----------|------------------------|-------------------------------------|----------------------------|
| Namecheap (or DNS) API key / hook env | **Yes** after S7b: secure note or login for reinstall comfort | **Yes** for install bridge source | Hook reads durable `/var/lib/surmount/secrets/acme/namecheap.env` by default (`/run/...` optional); mode 0600 |
| Stalwart admin / management API token **value** | **Yes** as human note (and engine still has its own principal) | **Yes** for install to token file | File e.g. durable `/var/lib/surmount/secrets/ui/stalwart-api-token`; engine registration is separate |
| Session HMAC secret | Optional human copy | **Yes** for reinstall | File under secrets root for management-ui |
| VW **ADMIN_TOKEN** itself | Optional offline paper/USB note; **not** "only inside VW on same host" as sole copy | **Yes** before first S7b enable | Durable `/var/lib/surmount/secrets/vaultwarden/admin.env` (`ADMIN_TOKEN=...`) |
| TLS PEMs (static path) | Usually **no** (large / awkward); backup offline if needed | **Yes** when not using ACME | cert/key under durable `/var/lib/surmount/secrets/tls/` (H-PEM) |
| ACME account JSON | Optional backup note | Optional | Written by binary under acme path; parents via `--ensure-acme-parents` |
| Age **admin** identity | Optional offline inventory pointer only | **Workstation only** (bridge refuses age-admin by default) | **Never** install admin age onto VPS casually |
| Age host / sops host key | Inventory note only | Bootstrap custody | Host path for sops-nix |
| restic password | **Yes** human copy | **Yes** | `backups.passwordFile` |
| Arti HS identity | Recovery checklist / who holds offline backup | Install package custody | `onionServiceStateDir` on host |
| Mail app passwords / TOTP / shared team logins | **Primary home** | n/a | Stalwart holds engine hashes; not VW |
| LUKS recovery | Offline only (not sole copy inside VW on same disk) | Offline | initrd unlock; not Secret Service alone |
| Edge hostnames / LE email | Optional notes | Optional notes | **Public-ish config** in host-local / Nix (not secrets-install kinds) |

**S7b enable path (already scripted; do not rebuild S7a):**

1. Put kind `vaultwarden-admin` in private staging (or Secret Service) with
   path `/var/lib/surmount/secrets/vaultwarden/admin.env` (durable default)
   and body `ADMIN_TOKEN=...` (or `host-cutover --generate-material
   --with-vaultwarden`).
2. `just host-cutover -- --dry-run --with-vaultwarden ...` then live when ready
   (private enable fragment + install + switch). Gates G8-G10 in [OPS.md](OPS.md).
3. Smoke: `systemctl is-active vaultwarden`; loopback Rocket; first admin UI
   account once. Console link when `managementUi.vaultwardenUrl` or derived
   loopback URL is set.
4. **Do not** edit or rewrite `modules/vaultwarden.nix` for day-1 enable; S7a
   is done. Sample host stays `enable=false` until private fragment / host-local.

**Explicit non-goals:**

| Do not | Why |
|--------|-----|
| Call VW / `bw` from systemd activation or flake eval | Chicken-egg; purity |
| Treat VW as Bitwarden **Secrets Manager** (`bws`) | VW is password manager only; **Q-SEC-SM-*** open |
| Keep the **only** copy of age admin, LUKS recovery, or restic password inside VW on the same VPS disk | Disk loss = total loss |
| Rebuild S7a module for enable | Use A0 cutover path |

### 5.2 Export from Vaultwarden to staging (optional; reinstall comfort)

When S7b is live and you keep reinstall material in VW, export selected items
into a **private staging tree**, then run `secrets-install-host` as usual.
Export is **optional**, operator-gated, and **never** required for boot.
**Not at boot / not free-443 activation:** free-443 and ACME still read Domain
B only after a separate install. Prefer first fill via
`just secrets-prompt` when the value is not yet in VW. Ops flow:
[OPS.md](OPS.md) *Vaultwarden human path*.

#### Manual path (docs-first; official Bitwarden CLI)

1. On the operator workstation, install official **`bw`** CLI (not `bws`).
2. Point the client at your Vaultwarden base URL; `bw login` / unlock (session
   stays local; do not paste session keys into git or chat).
3. For each reinstall item, pull the field you stored (examples are labels only):

```bash
# Labels only. Do not paste real values into chat, docs, or git.
bw get password 'surmount-session'           # -> staging session-secret/secret
bw get password 'surmount-stalwart-token'    # -> stalwart-token secret bytes
bw get notes 'surmount-namecheap-api'        # multi-line env file shape
bw get password 'surmount-vw-admin'          # wrap as ADMIN_TOKEN=... for VW kind
```

4. Write a private staging tree (outside git) in the layout from section 1.5
   (`attributes` + `secret` mode 0600). Set `surmount.host` to your logical
   host id and conventional `surmount.path` values from section 1.1.
5. Install domain B:

```bash
nix run .#secrets-install-host -- --from-staging /path/to/private-staging \
  --host-id YOUR_LOGICAL_HOST --target root@YOUR_HOST \
  --require-kind session-secret --require-kind stalwart-token
# add --require-kind vaultwarden-admin / other as needed
```

#### Optional thin helper (hermetic-testable; not activation)

`nix run .#secrets-export-bw-to-staging`
reads a private **map** tree (attributes only, including `surmount.bw_item` and
optional `surmount.bw_field` / `surmount.bw_wrap`) and writes install-ready
staging. Sources:

| Mode | Use |
|------|-----|
| `--from-fixture DIR` | Hermetic / offline practice; no live VW |
| `--from-bw` | Operator machine after unlocked `bw` session |

```bash
# Map + fixture practice (no network):
nix run .#secrets-export-bw-to-staging -- \
  --map /path/to/private-map --staging /path/to/private-staging \
  --host-id mail-lab --from-fixture /path/to/private-fixture

# Live export after bw unlock (operator only; never CI green):
nix run .#secrets-export-bw-to-staging -- \
  --map /path/to/private-map --staging /path/to/private-staging \
  --host-id YOUR_LOGICAL_HOST --from-bw

# Dry-run (kinds/paths only; no secret values logged):
nix run .#secrets-export-bw-to-staging -- \
  --map /path/to/private-map --staging /path/to/private-staging \
  --host-id mail-lab --from-fixture /path/to/private-fixture --dry-run
```

Map item sketch (no secret bytes in the map):

```text
private-map/vaultwarden-admin/attributes
  surmount.kind=vaultwarden-admin
  surmount.host=mail-lab
  surmount.path=/run/surmount-secrets/vaultwarden/admin.env
  surmount.bw_item=surmount-vw-admin
  surmount.bw_field=password
  surmount.bw_wrap=ADMIN_TOKEN
```

Self-test (CI / developer; fixture + mock `bw` only):

Hermetic export contracts live in crate tests (`checks.*.ci`; fixture + mock
`bw`; no live VW).

**Never** wire live Vaultwarden or live `bw login` as a green CI gate.

### Module defaults (when `surmount.vaultwarden.enable = true`)

| Setting | Default |
|---------|---------|
| Backend | SQLite (`dbBackend = "sqlite"`) |
| Data dir | `/var/lib/vaultwarden` (stock StateDirectory for stateVersion 26.05) |
| Listen | `127.0.0.1:8222` (ROCKET_ADDRESS / ROCKET_PORT) |
| Signups | `SIGNUPS_ALLOWED = false` (forced; invite via admin UI) |
| Admin token | `adminTokenEnvFile` host path (required when enable); unit `ConditionPathExists` |
| nginx | `configureNginx = false` (product refuses stock nginx VW edge) |

Example host path (comments only, never real secret in git):

```text
/run/surmount-secrets/vaultwarden/admin.env
# ADMIN_TOKEN=...   (KEY=value lines; mode 0600)
# kind: vaultwarden-admin
# install: nix run .#secrets-install-host or host-cutover --with-vaultwarden
```

Staging item sketch (private tree only; never git):

```text
private-staging/vaultwarden-admin/
  attributes    # surmount.kind=vaultwarden-admin
                # surmount.host=<logical-host-id>
                # surmount.path=/run/surmount-secrets/vaultwarden/admin.env
  secret        # ADMIN_TOKEN=...  (mode 0600 regular file)
```

Console link: set `surmount.managementUi.vaultwardenUrl` or leave empty so the
management UI derives `http://127.0.0.1:8222` when VW is enabled (honest
loopback / SSH tunnel path). Env: `SURMOUNT_VAULTWARDEN_URL`. JSON:
`vaultwarden_configured` + `vaultwarden_url` on `GET /api/v1/system`. No admin
token, nsec, or passwords in UI/API/logs.

### Data store choice (see DATASTORES.md)

| Engine | Recommendation |
|--------|----------------|
| **SQLite** | **Default for single VPS** |
| PostgreSQL | Only if already running PG or multi-instance VW |

Data directory must be on the restic path list when live. Client-side encryption
protects vault item payloads; the server DB and attachments still need backup
and disk hygiene.

### Chicken-and-egg

```text
  nixos-rebuild
       |
       v
  deploy secrets on host (domain B) --> /run/surmount-secrets/...
       |
       +--> stalwart-mail
       +--> restic
       +--> vaultwarden  (starts with host EnvironmentFile ADMIN_TOKEN)
       |
       v
  humans use Vaultwarden UI/API  (never required at eval time)
  management console shows Open vault when URL configured
```

### What Vaultwarden is not

- Not a replacement for encrypted deploy secrets (sops-nix or alternative).
- Not encryption for Stalwart/RocksDB mail.
- Not LUKS unlock.
- Not Nostr nsec storage (never put nsec in a shared vault casually; prefer
  hardware/client wallets and host OS key tools; if stored, highest sensitivity).
- **Not** the source of files for `nixos-rebuild` / unit start (use domain A -> B
  bridge; optional export to staging first when reinstalling).
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

## 6. Disk: LUKS2 FDE (optional third concern)

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

## 7. Product auth secrets (Nostr)

Product login is Nostr keys (npub + NIP-98 and sessions). Keys managed by host
OS via other Surmount tools; dead simple; not manual password management for
users. See operator-direction and SECURITY.

| Material | Where |
|----------|-------|
| **nsec** (private) | Client / OS key tools only; **never** server database; **never** git |
| npub allowlist (bootstrap Administrators) | `SURMOUNT_NOSTR_ALLOWLIST` or `SURMOUNT_NOSTR_ALLOWLIST_FILE` / Nix `managementUi.nostrAllowlist` + `nostrAllowlistFile` (env wins; Q-AUTH-1 bootstrap UX open). **Not** the people map. |
| Console account map (optional npub + role) | Host file `/var/lib/surmount/console/accounts.json` (or `SURMOUNT_CONSOLE_ACCOUNTS` / Nix `managementUi.consoleAccountsFile`). Owner `surmount-ui` mode **0600**. Versioned JSON list: optional mailbox, optional npub **hex**, role `administrator` or `user`. Unique npub. **Never nsec.** User npubs stay here and must **not** be copied to the host allowlist. Not in git. |
| Session signing keys / cookie secrets | Host-only `SURMOUNT_SESSION_SECRET` or EnvironmentFile via `managementUi.sessionSecretPath` (HMAC cookie scaffold; never git) |
| Stalwart management API token (directory list) | Host-only `SURMOUNT_STALWART_TOKEN` or raw file `SURMOUNT_STALWART_TOKEN_FILE` / Nix `managementUi.stalwartTokenPath` (Bearer for management JMAP; never git). Required only when `directory=stalwart` |
| Stalwart mail passwords / app passwords | Inside Stalwart directory (engine); human tracking in Vaultwarden |

### 7.1 One-shot: session-secret + nostr-allowlist (public edge B4)

Public services Host with the operator console **requires** mode=nostr and
these two host files (or equivalent env). Full runbook and proof curls:
[OPS.md](OPS.md) *Production Nostr auth on the public edge (B4)*. Posture:
[SECURITY.md](SECURITY.md).

**Never** commit secret bytes, real npubs you treat as sensitive, or nsec.
Placeholders only in docs and sample hosts.

#### session-secret

| | |
|--|--|
| **Kind** | `session-secret` |
| **Domain B path** | `/var/lib/surmount/secrets/ui/session-secret` (durable default) |
| **File shape** | systemd **EnvironmentFile**: single line `SURMOUNT_SESSION_SECRET=<value>` then newline; mode **0600** |
| **Consumer** | Nix `managementUi.sessionSecretPath` -> unit `EnvironmentFile=` -> env `SURMOUNT_SESSION_SECRET` |
| **Generate** | `just secrets-prompt -- session-secret --host YOUR_LOGICAL_HOST --generate` (writes **raw** 64 hex into private staging) |

```bash
# 1) Generate raw hex into private Domain A staging (outside public git tree).
just secrets-prompt -- session-secret --host YOUR_LOGICAL_HOST --generate

# 2) Wrap staging secret as EnvironmentFile before install (required for
#    sessionSecretPath). Example private edit (do not log the value):
#    printf 'SURMOUNT_SESSION_SECRET=%s\n' "$(cat staging/session-secret/secret)" \
#      > staging/session-secret/secret.wrapped && mv ...
#    Or write the KEY=value line by hand into the staging secret file.

# 3) Install Domain B (values never logged).
just secrets-install-host -- --from-staging "$HOME/.local/share/surmount/staging" \
  --host-id YOUR_LOGICAL_HOST --target root@YOUR_HOST \
  --require-kind session-secret
```

Lab-only: export `SURMOUNT_SESSION_SECRET=$(openssl rand -hex 32)` for
`just dev`, or Nix `sessionSecretEnv` (never production secrets in public tree).

#### nostr-allowlist

| | |
|--|--|
| **Kind** | `nostr-allowlist` (install bridge; **not** a secrets-prompt generate kind) |
| **Domain B path** | `/var/lib/surmount/secrets/ui/nostr-allowlist` (durable preferred) |
| **File shape** | One or more **npub** / hex pubkeys; comma, space, or newline separated; `#` comments OK; mode **0600**, owner **`surmount-ui`** |
| **Consumer** | UI process reads the file (`kind_needs_ui_owner`). Nix `managementUi.nostrAllowlistFile` -> `SURMOUNT_NOSTR_ALLOWLIST_FILE` (or short list via `nostrAllowlist` env string; **env wins** when non-empty) |
| **Fail-closed** | Empty allowlist + mode=nostr => nobody authenticates; gate still blocks anonymous console |

```bash
# Private staging layout (outside public git tree):
#   private-staging/nostr-allowlist/attributes
#     surmount.kind=nostr-allowlist
#     surmount.host=YOUR_LOGICAL_HOST
#     surmount.path=/var/lib/surmount/secrets/ui/nostr-allowlist
#   private-staging/nostr-allowlist/secret
#     npub1exampleplaceholder000000000000000000000000000000000000
#     # never put nsec here

just secrets-install-host -- --from-staging /path/to/private-staging \
  --host-id YOUR_LOGICAL_HOST --target root@YOUR_HOST \
  --require-kind nostr-allowlist
```

Or place the allowlist file on the host by hand (same path, mode 0600,
readable by `surmount-ui`), then point `nostrAllowlistFile` at it.

#### Private host-local (paths only; no secrets in public git)

```nix
# PRIVATE host-local fragment. Do not commit real npubs or secret values.
{
  surmount.managementUi = {
    authMode = "nostr";
    sessionSecretPath = "/var/lib/surmount/secrets/ui/session-secret";
    nostrAllowlistFile = "/var/lib/surmount/secrets/ui/nostr-allowlist";
    publicBaseUrl = "https://services.example.test";
  };
}
```

Sample public host (`hosts/mail-vps`) stays auth-off default with comments
only. Switch + live proof close residual **B4**; Q-AUTH-1 stays open.

---

## 8. Backup checklist for secrets-related material

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

## 9. Anti-patterns

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
| Treat laptop Secret Service as host activation SoT | Domain A custody; install material onto host (B) via bridge or manual |
| Expect headless VPS to query GNOME Keyring at boot | Place files / credentials on host before units start |
| Store deploy PEMs/age keys in the project tree | Host + operator channels only; never git |
| Claim GNOME / libsecret Rust crate is product-wired | Not wired; candidate + shell bridge + opt-in deploy glue only (S0-S4 offline; S5 parked) |
| Log secret values in install scripts or CI | Labels, kinds, paths, exit codes only |
| Install bridge write into public `hosts/` / `secrets/` | Refuse; use `/run/surmount-secrets/...` (or other host runtime paths) |
| Use live keyring or live host install as CI green | Hermetic `test-secrets-install-host.sh` / deploy-host self-test only |
| Soft-elevate S5/S6/S7b as shipped | S4 opt-in offline shipped (default off); A0 cutover pack offline shipped; S5 parked; S6 values on host still operator; S7a offline done; S7b driver-owned after token (live unit host-gated); V4 export optional only |
| Call VW / bw from activation or CI green | Domain A/B bridge only at boot; export is optional reinstall path; hermetic fixture/mock tests only |
| Rebuild S7a for host enable | Use A0 `--with-vaultwarden` + private fragment |

---

## 10. Related modules and scripts

| Path | Role |
|------|------|
| `modules/secrets.nix` | sops-nix defaults; `requireDeployMaterial` |
| `modules/backups.nix` | restic password file hook |
| `modules/mail.nix` | Stalwart credentials comments |
| `modules/vaultwarden.nix` | Domain C module (S7a offline; do not rebuild) |
| `secrets/README.md` | Operator bootstrap (host-local; nothing secret committed) |
| `nix run .#secrets-install-host` | Domain A -> B install bridge (staging / optional secret-tool; kinds include `namecheap-api`, `stalwart-recovery-admin`) |
| `nix run .#stalwart-recovery-unlock` | Generate/install/strip recovery admin pin (kind `stalwart-recovery-admin` + unit/durable EnvironmentFile path + probe; prefer `--strip` after mint) |
| `nix run .#surmount-fix-public-dashboard` | Compose recovery unlock + bootstrap API token + free-443 dry-run + optional `--strip-recovery` |
| `nix run .#free-stalwart-public-443` | Free Stalwart public :443 using Domain B `stalwart-token` (default dry-run) |
| `nix run .#secrets-export-bw-to-staging` | Optional C -> A staging export (fixture or `bw`; never activation) |
| `nix run .#surmount-deploy-host` | Public tree sync + host-local; **opt-in** `--install-secrets` (S4) calls the bridge before rebuild; default off |
| `nix run .#surmount-host-cutover` | A0+V5 compose (inventory, install plan, ACME parents, optional `--host-profile` / `--free-443` / VW enable, smoke) |
| crate tests in `checks.*.ci` | Hermetic red/green contracts for those bins (no live keyring/host) |
