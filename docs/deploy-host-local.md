# Host-local overlay contract (deploy)

How operators keep **box-specific** material off the public Surmount Server
tree while still rebuilding `#mail-vps` repeatably.

**Last updated:** 2026-09-20 (operator procedure through dry-run is
agent work; real switch is operator. Full list:
[TEST-AND-DEPLOY.md](TEST-AND-DEPLOY.md)). Prior 2026-08-25 (driver is `nix run .#surmount-deploy-host` /
`just deploy-host`; crate tests in `checks.*.ci`. No product `nix run .#surmount-deploy-host --`.)
**Companions:** [OPS.md](OPS.md) (cutover gates), [hygiene.md](hygiene.md),
[SECRETS.md](SECRETS.md), [SECURITY.md](SECURITY.md), sample
[hosts/mail-vps/configuration.nix](../hosts/mail-vps/configuration.nix).
**Driver:** `nix run .#surmount-deploy-host` / `just deploy-host` (crate
`surmount-deploy-host`; Nix package `nix/packages/surmount-deploy-host.nix`).
Post-switch smoke is the same crate (`surmount-deploy-host-post-switch-smoke`).
**Secrets install bridge:** `nix run .#secrets-install-host` (crate
`surmount-secrets-install`). Full three-domain model: [SECRETS.md](SECRETS.md)
section 1. **Opt-in S4:** deploy driver `--install-secrets` may invoke the
bridge before rebuild (default off).
**Host cutover compose (A0):** `nix run .#surmount-host-cutover` / `just
host-cutover` (default dry-run; inventory + optional fragments + install/deploy
plan + smoke). Private fragment emit only (never writes `enable = true` into
public `hosts/`).

This is living ops contract, not a claim that a given production host is
already wired. Local `just e2e` is never cutover.

---

## 1. Why host-local exists

The public repo ships a **generic** sample host:

| Public tree | Stays generic |
|-------------|---------------|
| Flake attr | `#mail-vps` (alias `#surmount-mail`) |
| Path | `hosts/mail-vps/configuration.nix` |
| Default hostname | `lib.mkDefault "mail-vps"` |

Real machines need material that **must not** land in public git:

- Hardware / disk layout (`hardware-configuration.nix` or disko)
- SSH authorized public keys (long pubs)
- Static networking and real public addresses
- Real OS hostname
- TLS PEMs, age identities, Arti HS keys, Stalwart API tokens

**Split (honest):** host-local is the **Nix / SSH / network overlay**. Prefer
**domain A** (GNOME Secret Service or private staging) plus
`nix run .#secrets-install-host` for **secret file** material into domain B
runtime paths (e.g. `/run/surmount-secrets/...`). Host-local may still hold
path pointers and non-secret hardware; do not treat host-local as a second
git-adjacent secrets dump. Schema and runbook: [SECRETS.md](SECRETS.md).

Standing law: [hygiene.md](hygiene.md) (never secrets; host identity off tree),
[SECRETS.md](SECRETS.md). Scanner: `surmount-private-data`.

---

## 2. Required overlay pieces

Operators maintain a **private** directory (on the host and/or a private
operator path). Conventional name on the machine after first deploy:

```text
/root/surmount-server/host-local/
```

The deploy driver syncs public tree separately and may rsync this private
dir to `REMOTE/host-local/` only (never into tracked `hosts/` or `secrets/`).

| Piece | Role | In public git? |
|-------|------|----------------|
| `hardware-configuration.nix` (or disko output) | Real disks / initrd modules | **No** |
| `authorized_keys` (and/or `*.pub`) | Root and/or `surmount` SSH access before password auth off | **No** |
| Networking snippet | Static IPv4/IPv6, gateway, or deliberate DHCP | **No** (real addresses) |
| Hostname override | Real `networking.hostName` (higher priority than sample `mkDefault`) | **No** (real box names) |
| Optional: sops/age path notes | Where host age key lives (`/var/lib/sops-nix/key.txt`) | Paths only in docs; keys **No** |
| Optional: TLS / Arti / token paths | Under `/run/surmount-secrets/...` on host | Material **No** |

Sample host comments already list first-deploy steps; this doc is the
contract the **deploy driver** and operators share.

### Forbidden in public git (reminder)

| Forbidden | Notes |
|-----------|--------|
| PEMs, age secret keys, tokens, `.env` | [hygiene.md](hygiene.md) |
| Long SSH public keys under `hosts/` / `secrets/` | Scanner path-gated |
| Real public IPv4 under `hosts/**` | TEST-NET / RFC1918 docs ranges only |
| First-deploy box hostname as flake attr or committed path | Keep `#mail-vps` |
| Copying `host-local/` into tracked `hosts/mail-vps/` | Driver **refuses** |

---

## 3. Example layout (placeholders only)

**Synthetic only.** TEST-NET addresses and short comments. Never paste real
keys, IPs, or box names into the public tree or into committed fixtures
outside `script/testdata/` detector samples.

```text
host-local/                          # private; not in public git (/host-local/ gitignored)
  default.nix                        # optional: full module entry (if present, flake imports only this)
  hardware-configuration.nix         # from nixos-generate-config on the box
  authorized_keys                    # one or more real ssh-ed25519 lines (not a .nix file)
  networking.nix                     # optional module fragment
  hostname.nix                       # optional module fragment
  host-local-acme.nix                # optional: ACME block from private host profile render
```

**Flake auto-import (deploy-ready):** `flake.nix` only composes host modules.
When `./host-local` exists at the flake root (after `deploy-host` rsyncs private
material to `REMOTE/host-local/`), `#mail-vps` includes it:

| Presence | What is imported |
|----------|------------------|
| No `./host-local` | Nothing (public tree eval identical to today) |
| `host-local/default.nix` | That directory/module only (operator owns full compose) |
| Else known pieces that exist | `hardware-configuration.nix`, `hostname.nix`, `networking.nix`, `host-local-acme.nix` |
| `host-local/authorized_keys` (no `default.nix`) | Wires `users.users.root` and `users.users.surmount` `openssh.authorizedKeys.keyFiles` |

**Host profile -> ACME fragment (V2, non-secrets):** edge domains and LE email
live in a private profile (suggested
`~/.local/share/surmount/host-profile.toml`), **not** in Domain A secrets and
**not** as Vaultwarden activation. Render:

```bash
just render-host-profile-acme -- \
  --profile ~/.local/share/surmount/host-profile.toml \
  --out /path/to/private/host-local
# writes host-local-acme.nix (refuses public git tree paths)
```

Public sample (placeholders only):
`script/testdata/host-profile/sample-host-profile.toml`.
If you use `host-local/default.nix`, import `./host-local-acme.nix` yourself
(known-name auto-import is skipped when `default.nix` is present).

Known names only (no wildcard `*.nix`) so stray files cannot surprise switch.
Public git never requires host-local; root `.gitignore` has `/host-local/`.

**Why path: rebuild:** deploy-host rsyncs the public tree **without** `.git/`,
then rsyncs host-local beside it. Remote rebuild uses
`nixos-rebuild switch --flake path:REMOTE_DIR#mail-vps` so Nix treats the tree
as a **path flake** and sees gitignored/untracked host-local. A git flake URI
on a checkout that still has `.git` would omit those paths.

Live deploy still needs an operator-supplied target, working SSH, and real keys
that match the agent / workstation (never invented in this tree).

Example **shape** of `networking.nix` (TEST-NET only):

```nix
# PRIVATE host-local only. Do not commit.
{
  networking.useDHCP = false;
  networking.interfaces.eth0.ipv4.addresses = [{
    address = "203.0.113.10";   # TEST-NET-3 documentation range
    prefixLength = 24;
  }];
  networking.defaultGateway = "203.0.113.1";
}
```

Example **shape** of `hostname.nix`:

```nix
# PRIVATE host-local only. Do not commit real box names to public git.
{ lib, ... }:
{
  networking.hostName = lib.mkForce "your-private-hostname";
}
```

Example **shape** of `authorized_keys` (comment placeholder in public docs;
real lines only on host):

```text
# ssh-ed25519 AAAA... you@workstation   # paste real key on host only
```

Optional `host-local/default.nix` when you want one entry module (imports the
pieces yourself and must wire authorized keys if you use that path). The sample
public `hosts/mail-vps/configuration.nix` stays generic.

### B0: keys must be in evaluated config (necessary but not sufficient)

The deploy driver **fails loud** if host-local has no usable SSH public key
file. That check is **necessary but not sufficient** for lockout safety:

| Layer | Role |
|-------|------|
| Host-local `authorized_keys` / `*.pub` files | What the driver scans before switch |
| **Evaluated** NixOS config | What actually becomes `/etc/ssh/authorized_keys` after switch: `keyFiles` / `keys` on root and/or `surmount` |

With the known-name layout (no custom `default.nix`), flake auto-import sets
`authorizedKeys.keyFiles` from `host-local/authorized_keys` on path: rebuild.
If you ship a custom `default.nix`, you must import keys yourself; otherwise
password auth off still means **lockout**. The driver prints this note after a
passing key-file check.

---

## 4. Deploy driver

```bash
# Dry-run (no SSH side effects): print rsync + rebuild plan
nix run .#surmount-deploy-host -- --dry-run --target root@example.test \
  --host-local /path/to/private/host-local

# Env form
export SURMOUNT_DEPLOY_TARGET=root@example.test
export SURMOUNT_HOST_LOCAL_DIR=/path/to/private/host-local
nix run .#surmount-deploy-host -- --dry-run

# Thin just wrapper (same script; positional args, safe quoting)
just deploy-host --dry-run --target root@example.test \
  --host-local /path/to/private/host-local

# Opt-in secrets install before rebuild (S4; default off)
nix run .#surmount-deploy-host -- --dry-run --target root@example.test \
  --host-local /path/to/private/host-local \
  --install-secrets --secrets-staging /path/to/private-staging \
  --secrets-host-id mail-lab --secrets-require-kind tls-cert
```

| Behavior | Detail |
|----------|--------|
| Target | **Required** (`--target` or `SURMOUNT_DEPLOY_TARGET`). No committed real host default. Rejects values starting with `-` (OpenSSH option-shaped). |
| Host-local check | Requires dir; **fails loud** if no usable SSH public key (lockout with password auth off). **Not sufficient alone:** keys must also be in evaluated NixOS config (see B0 above). |
| Public sync | rsync of public tree; excludes `host-local/`, `.git`, `crates/target`, secret basenames (`.env`, `*.pem`, `*.agekey`, `secrets.yaml`, `secrets/`, SSH private key names, ...). **Not** a substitute for `surmount-private-data` admission control. |
| Host-local remote | Optional rsync to `REMOTE/host-local/` only; **refuses** `--sync-host-local-into` under `hosts/` or `secrets/` (path segments, missing parents, and symlink targets). Default **without** rsync `--delete`; pass `--host-local-delete` to opt in (incomplete local can wipe remote-only files if delete is on). |
| `--skip-host-local-check` | Skips key-file checks **and** remote host-local rsync (documented side effect; not recommended). |
| **`--install-secrets` (S4)** | **Opt-in only.** Validates source/host-id/staging **before** any rsync; runs `nix run .#secrets-install-host` after sync and before rebuild. Staging must be **outside** the public git work tree (refuse in-tree so public rsync cannot copy payloads). Default **off**. Never logs secret values. See [SECRETS.md](SECRETS.md). |
| Rebuild | `nixos-rebuild switch --flake path:REMOTE_DIR#mail-vps` on the target (on-host build; absolute path: so host-local is visible). Uses `ssh --` so the host is never parsed as an SSH option. |
| Post-switch smoke | After rebuild returns (even when rebuild exit is non-zero), always runs `surmount-deploy-host-post-switch-smoke` on the remote via SSH: current generation pointer, `systemctl is-active` for `sshd` / `stalwart-mail` / `surmount-management-ui` (starts UI if inactive), loopback `/health` (day-1 default `http://127.0.0.1:8090/health`; when unit `SURMOUNT_LISTEN_MODE=https` and the listen port is 443, proven loopback is `https://127.0.0.1:443/health` with `curl -sk`, not HTTP on :443; port may follow unit `SURMOUNT_LISTEN` when parseable; never invents a public URL), TLS PEM path presence when https edge or `SURMOUNT_TLS_*` is set (durable default `/var/lib/surmount/secrets/tls/{cert,key}.pem`; path labels only, never PEM contents), optional ACME enable note from `SURMOUNT_ACME_ENABLE`, and when https edge is configured, `ss` listen proof for `:443` and `:80`. Day-1 loopback/http skips PEM and public listen checks. Clear PASS/FAIL lines. Self-test: crate tests in `checks.*.ci`. |
| Dry-run | Prints commands (including planned secrets-install when opted in, and post-switch smoke); hermetic tests use this path. |
| Deeper smoke | Optional `just e2e-host` / DNS/TLS/mail scripts; does not claim cutover. |

Self-test (synthetic fixtures only):

Hermetic contracts live in crate tests (`checks.*.ci`; fake
systemctl/curl/ss; no live SSH).

Day-one compose (prefer over hand-walking install + deploy when material is ready):

```bash
# Dry-run default; staging and host-local stay outside public git
just host-cutover -- --dry-run \
  --staging /path/to/private-staging --host-id mail-lab \
  --target root@example.test --host-local /path/to/private/host-local \
  --emit-fragments /path/to/private/host-local

# S7b profile (VW admin token + enable fragment + smoke recipes)
just host-cutover -- --dry-run --with-vaultwarden \
  --staging /path/to/private-staging --host-id mail-lab \
  --target root@example.test --host-local /path/to/private/host-local \
  --emit-fragments /path/to/private/host-local
```

CI stays hermetic: private-data + script self-tests + `just ci`. No auto SSH
switch from GHA. Live LE / live keyring / live SSH never green CI.

---

## 5. Activation reliability (first switch)

First production-ish switches have sometimes **appeared to hang** after
activation messages around **reloading user units for root** (`user@0`).

| Item | Honesty |
|------|---------|
| Status | **Known measured ops residual** on at least one first deploy; not a claimed product bug with a red in-tree test |
| Impact | Operator may wait a long time or Ctrl-C; system may still be mid-switch; SSH session that ran rebuild may look stuck |
| Prefer | **Second SSH session** open before risky switches so you can inspect and recover without losing the only shell |
| Workaround (operator) | Wait for switch to finish; if stuck, from another session: `systemctl restart user@0.service` and/or inspect `systemctl list-jobs` / switch locks under `/run/` for nixos-rebuild; re-run `nixos-rebuild switch --flake path:REMOTE_DIR#mail-vps` (or `.#mail-vps` on a checkout) once services are up |
| After switch returns | **deploy-host** always runs post-switch smoke on the remote (even if rebuild exit was non-zero: activation may still have applied). Verify **generation current** + units active + loopback `/health`; on https edge also PEM paths + `:80`/`:443` listen. Re-run alone: `nix run .#surmount-deploy-host-post-switch-smoke` on the host |
| Do not | Invent a Surmount module "fix" without a reproducible failure; do not force-kill blindly if you still have only one SSH session |

Password auth off + empty authorized keys is a separate lockout class (driver
checks keys when host-local is provided).

---

## 6. Security posture ladder (host work; proof gates)

Tree modules already ship most capabilities. **Sample host defaults leave
HTTPS PEMs, `requireDeployMaterial`, Arti, ban enforce, and production Nostr
commented** so eval stays free of host material. Enabling them is
**operator host work**, not a free sample default.

| Stage | Goal | Proof (host) | Tree knobs (already present) |
|-------|------|--------------|------------------------------|
| **B0** | Preserve access | SSH keys in host-local **and** in evaluated NixOS config before password-off switch (driver file check is necessary but not sufficient) | `users.users.*.openssh.authorizedKeys`; hardening password default |
| **B0+ UI** | Cleartext management UI not public | Until B1 PEMs + Axum :443: `managementUi.listenAddress = "127.0.0.1"` (port default 8090); prove `ss` loopback-only + public :8090 closed. Default `authMode=off` is not public-safe. Pin in **private host-local** | `surmount.managementUi.listenAddress` / `port` ([options.nix](../modules/options.nix)) |
| **B1** | Public HTTPS management UI | PEMs on host; `listenMode=https`; free :80 redirect; `just e2e-host` | `managementUi.*` PEM paths; [EDGE_AND_TLS.md](EDGE_AND_TLS.md) |
| **B2** | Deploy-material fail-loud | Activation fails if required paths missing | `secrets.requireDeployMaterial` + `requiredHostPaths` |
| **B3** | Arti management onion | HS dir owned `surmount-arti`; enable + startDaemon; hostname file + live Tor verify | `artiHiddenService.*` |
| **B4** | Production Nostr auth on **public** edge | Domain B session-secret (EnvironmentFile) + nostr-allowlist; private host-local `authMode=nostr` + paths + `publicBaseUrl`; prove anonymous services login/401 and health 200; auth-off is loopback/lab only. Runbook: [OPS.md](OPS.md) B4 | `managementUi.authMode` / `sessionSecretPath` / `nostrAllowlistFile` / `publicBaseUrl` |
| **B5** | Merciless ban lab | nft + helper + enforce; traffic drop proof | `accessControl.*` (default off) |
| **B6** | Mail legitimacy | SPF/DKIM/DMARC/PTR + Stalwart TLS; [DNS.md](DNS.md) earn-trust checklist | Registrar/managed DNS; not self-host DNS required. Day-1 A/AAAA/MX/PTR order (edge before send-auth): [DNS.md Day-1](DNS.md#day-1-dns-checklist-operator) |
| **B7** | Shrink transitional Python | Retire fail2ban sshd when Rust path covers SSH | After B5 solid; [SECURITY.md](SECURITY.md) |

Do **not** enable B1-B5 on the sample host in public git without material
(breaks honest eval / pretends cutover). Open Q-* stay open
([open-choices.md](open-choices.md), [RESIDUAL.md](../RESIDUAL.md)).

### D1 TLS hybrid KEX (with/after B1)

Management-ui uses **rustls + aws-lc-rs** with workspace feature
`prefer-post-quantum` so the default provider offers hybrid **X25519MLKEM768**
first among kx groups. Hermetic unit tests pin that provider config. That is
**not** automatic proof that a live host negotiated hybrid today.

| Claim | Status |
|-------|--------|
| Code path uses aws-lc-rs / TLS 1.3 lean | Shipped in tree |
| Hybrid group in default provider (includes X25519MLKEM768) | **Hermetic unit tests** (`aws_lc_rs_default_provider_includes_*` / `prefers_*`) |
| Prefer hybrid first (`prefer-post-quantum`) | **Shipped** workspace rustls feature + prefer-first unit test |
| Live host hybrid negotiation | **Host residual** until measured after B1 (probe below) |
| PQConnect (D2) | Separate path-layer track; human-owned sibling packaging; not B1 |

Detail: [EDGE_AND_TLS.md](EDGE_AND_TLS.md),
[research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).

#### D1-host operator runbook (after B1 public HTTPS)

**Prerequisites:** B1 done (PEMs on host, `listenMode=https`, management UI
reachable on HTTPS). Do **not** treat offline `cargo test ... tls::` or
`just e2e` as host hybrid proof.

**Env placeholders only.** Never commit real hostnames, public IPs, or PEMs.

1. Confirm public HTTPS health (same env family as host e2e):

```bash
export SURMOUNT_E2E_BASE_URL=https://example.test:443
# optional: export SURMOUNT_TLS_SERVERNAME=example.test
curl -fsS "${SURMOUNT_E2E_BASE_URL%/}/health"
```

2. Hybrid negotiation probe (openssl on PATH; OpenSSL 3.x with hybrid
   groups preferred). Script exits **2** with **BLOCKED** when
   `SURMOUNT_E2E_BASE_URL` is unset (no silent skip success):

```bash
export SURMOUNT_E2E_BASE_URL=https://example.test:443
nix run .#surmount-tls-hybrid
# or: just check-tls-hybrid
```

| Exit | Meaning |
|------|---------|
| **0** | openssl reported a negotiated group containing **MLKEM** (hybrid) |
| **1** | handshake failed, classical-only group, or group line unparsed (honest fail) |
| **2** | **BLOCKED** (`SURMOUNT_E2E_BASE_URL` unset) |
| **127** | openssl missing |

3. Manual openssl equivalent (if you prefer not to use the script):

```bash
# Placeholders only. Reports "Negotiated TLS1.3 group: ..." when supported.
echo | openssl s_client \
  -connect example.test:443 \
  -servername example.test \
  -tls1_3 \
  -groups X25519MLKEM768:SecP256r1MLKEM768:X25519:secp256r1 \
  2>&1 | grep -E 'Protocol:|Negotiated TLS|Cipher is'
```

4. Offline provider re-check (not host negotiation):

```bash
cd crates && cargo test -p surmount-management-ui --bin surmount-management-ui tls::
```

**Honesty:** a green hybrid probe means *this client* negotiated hybrid with
*that* live process. It does not answer Q-PQC-*, mail-plane PQ, or PQConnect
(D2). Hermetic self-test for the probe gate:
crate tests in `checks.*.ci`. **Not** in
flake `checks.ci`.

### D2 PQConnect (offline prep only; do not mash with D1)

| Item | Status |
|------|--------|
| Sibling packaging | Human-owned commit in sibling `pqconnect` checkout |
| Flake path input in surmount-server | **Not wired** by default until human pin (plan default) |
| Host keys / DNS announce | Host-only; never git |
| Rebuild before readiness | Living host channel **nixos-26.05**; do not claim ready on 25.05 alone |
| Q-PQC-1..4 | Open; do not invent |

Consume sketch and residual: [research/pqconnect-local-packaging.md](research/pqconnect-local-packaging.md),
[research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md),
[RESIDUAL.md](../RESIDUAL.md).

---

## 7. Related paths

| Path | Role |
|------|------|
| [hosts/mail-vps/configuration.nix](../hosts/mail-vps/configuration.nix) | Generic sample + checklist comments |
| `nix run .#surmount-deploy-host` | Operator deploy driver (sync + rebuild + post-switch smoke) |
| `nix run .#surmount-deploy-host-post-switch-smoke` | On-host post-switch smoke (generation, units, loopback /health, PEM paths, ACME note, :80/:443 listen) |
| crate tests in `checks.*.ci` | Hermetic fake-systemctl/curl/ss self-test for smoke |
| `nix run .#surmount-private-data` | Admission control |
| `nix run .#surmount-tls-hybrid` | D1 host hybrid negotiation probe (env-gated) |
| [OPS.md](OPS.md) | Day-one bring-up order |
| [RESIDUAL.md](../RESIDUAL.md) | Host-gated residual honesty |
