# Test and deploy procedure (mail host)

This is the full procedure for this repository. Do not treat a chat
summary as a substitute. Commands below are complete. The **agent**
runs every command in sections 2, 3, and 4 from the laptop clone,
including the deploy dry-run. The **operator** does not run eval,
`just check-remote`, or the dry-run in order to discover failures.
Those failures belong on the agent. The operator runs the real switch
in section 5 after the agent has already reported those gates green,
and keeps Eternal Terminal open so activation cannot strand the box.

**Last updated:** 2026-09-20.

Companions: [OPS.md](OPS.md), [deploy-host-local.md](deploy-host-local.md),
[SECRETS.md](SECRETS.md), [hygiene.md](hygiene.md), [../AGENTS.md](../AGENTS.md).

---

## Why the operator still types the real switch

The agent can and must run the entire procedure **up to and including**
`just deploy-host -- --dry-run`. Named eval, `just check-remote`, clippy,
nixfmt, and the dry-run plan are how errors show up. Dumping those
commands on the operator is a broken leftover. Standing law:
[../AGENTS.md](../AGENTS.md) *Deploy dry-run is agent work* and *Prove
the operator gate*.

The operator still runs the **real** `just deploy-host` (no `--dry-run`)
because:

1. Agents are forbidden from the real switch unless the operator
   overrides that standing rule in writing. Dry-run is required agent
   work. The switch is not.
2. `nixos-rebuild switch` is the live generation: it stops and starts
   `surmount-management-ui`, `surmount-scram`, and related units. A
   failed or hung switch is a live mail-host event, not a CI log.
3. Activation can drop the SSH session that started the switch
   (`reloading user units for root`). The operator already sits in
   Eternal Terminal for that. The agent does not own that TTY.
4. The switch applies private host-local (hardware, keys, Splora
   instance enable, MTA-STS mode). The agent must not treat that as a
   laptop cargo proof.

Secrets custody (cookie files, Namecheap if the agent cannot see env
mode 0600, GPG sign) stays operator-owned for the same prove-the-gate
reason. Eval, remote CI, and dry-run are not secrets custody.

---

## 0. Who does what

| Who | Does | Does not |
|-----|------|----------|
| Agent | **All of sections 2, 3, and 4**, including every named `nix eval` and `just check-remote` and `just deploy-host -- --dry-run`. Report pass or fail with the store path or the named error. GitHub issues on plan Approve and on bug reports (screenshots attached). GitHub pull request after the operator has signed and pushed a git-flow feature branch. Laptop Let's Encrypt `--issue` when this laptop already has the Namecheap env files (do not print the env). | Laptop `cargo`, `rustc`, `BUILD_LOCAL=true`. Real `just deploy-host` switch unless the operator overrides. `git commit`, GPG sign, `git push`. Reboot. Invent secrets, cookie bytes, or JSON-RPC addresses. Asking the operator to run eval or `check-remote` so the operator can see the error. |
| Operator | Eternal Terminal window before a switch. Real `just deploy-host` (no `--dry-run`) after the agent reports dry-run green. GPG-signed commit and `git push` on a git-flow `feature/` or `bugfix/` branch. Place cookie files and other Domain B secrets the agent cannot create. | Run section 2, 3, or 4 in order to discover CI or eval failures. Leave the only SSH session as the one that started `nixos-rebuild switch`. |

Laptop = this session host. It is not the remote builder.

surmount-1 = mail guest and the allowed remote Nix builder (ssh-ng as
`nixbuilder`). Mail and builder share that guest. Do not nice mail.
Do not publish guest RAM, disk, SKU, or IP in git or chat.

Private overlay on the laptop (never git):

```text
/home/hunter/.local/share/surmount/host-local
```

After a successful switch, the guest copy is `/root/surmount-server/host-local/`.

---

## 1. Open Eternal Terminal first (operator, before the real switch only)

The agent does not need Eternal Terminal to run eval, `just check-remote`,
or the dry-run. Those use the laptop Nix client and ssh-ng as
`nixbuilder`, or a non-interactive deploy dry-run.

Activation can drop the SSH that started the **real** switch. The
operator opens a second window and leaves it logged in **before section
5**, not before section 2.

From the laptop home justfile (`~/justfile` wrapping this clone) or from
this clone:

```bash
just et
```

On the guest, for a switch that must survive a dead laptop tunnel:

```bash
tmux new -s switch
```

Run the later switch command inside that tmux session if you are already
on the guest. From the laptop, still keep `just et` open.

Confirm the builder answers before quality work:

```bash
ssh -o BatchMode=yes -o ConnectTimeout=8 nixbuilder@surmount-1 'echo nixbuilder_ok; hostname'
```

Expected: `nixbuilder_ok` and hostname `surmount-1`.

Do not start a second `just check-remote` while one is live. Do not start
Lake from an agent.

---

## 2. Named flake eval (agent; no rustc on this laptop)

These are Nix eval contracts. They do not compile the mail UI with
laptop rustc. The **agent** runs all four from the repo root and
reports the store path or the error. The operator does not run them to
find out they failed.

Use this laptop's flake system (`x86_64-linux` on this machine). If
`nix eval --impure --raw --expr 'builtins.currentSystem'` prints
something else, the agent substitutes that string for `x86_64-linux` in
every command in this section.

### 2.1 Module and host-contract eval

```bash
nix eval --raw '.#checks.x86_64-linux.module-eval-contract'
```

Success prints a store path whose name ends in `-module-eval-contract`.
Failure is a Nix assert or a `writeText` error. Fix the named contract
in `tests/module-eval.nix`. Do not skip it.

### 2.2 Deploy-secrets path charset

```bash
nix eval --raw '.#checks.x86_64-linux.deploy-secrets-contract'
```

Success prints a store path ending in `-deploy-secrets-contract`.

### 2.3 Arti module path shape

```bash
nix eval --raw '.#checks.x86_64-linux.arti-module-contract'
```

Success prints a store path ending in `-arti-module-contract`.

### 2.4 Splora package pass-through (no RocksDB build)

```bash
nix eval --raw '.#checks.x86_64-linux.splora-package-contract'
```

Success prints a store path ending in `-splora-package-contract`.
This does not build `pkgs.splora` or `splora-deps`. It only checks the
flake-input package attrs and Menhera cargo config on the input tree.

Run 2.4 every time `flake.lock` input `splora` moved. Run 2.1 through
2.3 every time modules, management-ui Nix env, or tests/module-eval.nix
changed. When in doubt, run all four.

Do not use `BUILD_LOCAL=true` on these commands.

---

## 3. Remote quality bar (`just check-remote`) (agent)

This is the CI aggregate this repo actually gates on. It is not grok-oss
`just test-remote`. This repo has no `just test-remote` recipe. The
**agent** runs it. A clippy or nixfmt failure is an agent fix, not an
operator homework list.

From the repo root, one live run at a time:

```bash
just check-remote
```

What that does:

- Refuses `BUILD_LOCAL` (exit 2). That would rustc on the laptop.
- Prints that it is building `.#checks.<system>.ci` with `--option max-jobs 0`.
- Flake eval can sit for several minutes with no rustc. That is expected.
- Then Nix builds on nixbuilder (ssh-ng). `-L` streams remote logs.
- On success it prints `just check-remote: ok` and a store path for
  `surmount-ci`.

The aggregate `checks.<system>.ci` constituents are:

- `packages.management-ui` (the UI binary)
- `management-ui-test`
- `management-ui-clippy` (`-D warnings`)
- `management-ui-fmt`
- `e2e-pure-test`
- `e2e-clippy`
- `module-eval-contract`
- `deploy-secrets-contract`
- `arti-module-contract`
- `arti-onion-package-contract`
- `public-site-package-contract`
- `splora-package-contract`
- `nix-fmt-check`
- `cargo-deny-bans`
- `cargo-audit`
- plus `wave1CiExtras` from `flake.nix` when that list is non-empty

Not in `ci` (do not treat these as the quality bar):

- `just e2e-host` (needs `SURMOUNT_E2E_HOST=1` and `SURMOUNT_E2E_BASE_URL`)
- mail-vm / Stalwart FOD / `#mail-vps` toplevel
- starting Splora indexers
- the real `nixos-rebuild switch`

If `just check-remote` fails, the log names a derivation (`nixfmt
failed: PATH`, clippy file:line, a failed test). Fix that contract. Do
not `#[allow]` to shrink the list. Do not fit tests to code. Re-run
`just check-remote` until it prints `ok` and a `surmount-ci` store path.

Optional same remote path for audit only:

```bash
just audit-remote
```

Laptop `just check` / `just ci` without `check-remote` would rustc here.
Do not use those as the mail-host quality bar.

---

## 4. Deploy dry-run (agent; required before any real switch)

The **agent** runs this. Agents must have already run section 2 and
section 3, then this dry-run, before asking the operator to switch.
The dry-run does not switch. A dry-run failure (bad host-local, missing
keys, rsync refuse) is an agent problem until it is green.

From the repo root:

```bash
just deploy-host -- --dry-run --target root@surmount-1 --host-local /home/hunter/.local/share/surmount/host-local
```

Equivalent without just:

```bash
nix run .#surmount-deploy-host -- --dry-run --target root@surmount-1 --host-local /home/hunter/.local/share/surmount/host-local
```

The extra `--` after `just deploy-host` stops just from eating flags.

What the dry-run prints (it must not rebuild the guest):

- rsync of the public tree to `root@surmount-1:/root/surmount-server/`
  (excludes `.git/`, `host-local/`, secrets patterns)
- rsync of private host-local to
  `root@surmount-1:/root/surmount-server/host-local/`
- `nixos-rebuild switch --flake path:/root/surmount-server#mail-vps`
  as a planned remote command
- post-switch smoke as a planned remote command

Success is a printed plan and exit 0. It is not a new generation.

Host-local never comes from public git. The flake on the guest is
`path:` so untracked host-local is visible to Nix.

Opt-in secrets install before rebuild (default off):

```bash
just deploy-host -- --dry-run --target root@surmount-1 \
  --host-local /home/hunter/.local/share/surmount/host-local \
  --install-secrets --secrets-staging /path/to/private-staging \
  --secrets-host-id YOUR_LOGICAL_HOST \
  --secrets-require-kind tls-cert --secrets-require-kind tls-key
```

Do not pass `--install-secrets` unless you mean to copy Domain B secrets
in that same deploy. Schema: [SECRETS.md](SECRETS.md).

---

## 5. Real switch (operator only)

This is the first command in this procedure that the operator must type
for the generation to go live. Eternal Terminal from section 1 must
already be open. The agent has already reported section 2, 3, and 4
green, including the exact dry-run command and exit 0.

```bash
just deploy-host -- --target root@surmount-1 --host-local /home/hunter/.local/share/surmount/host-local
```

That rsyncs, then runs `nixos-rebuild switch` on the guest. A generation
that compiles grok-oss or Splora can sit on one `building ...drv` line
for a long time. Check guest rustc before killing it.

After rebuild, the driver always runs
`surmount-deploy-host-post-switch-smoke` on the guest, even if rebuild
exit is non-zero.

Smoke must pass all of:

- current NixOS generation store path
- `sshd` active
- `stalwart-mail` active
- `surmount-management-ui` active (smoke may start it if inactive)
- loopback `https://127.0.0.1:443/health`
- TLS cert and key paths exist; key mode 600
- listen proof for :443 and :80 (`ss`)

Re-run smoke only:

```bash
ssh -- root@surmount-1 surmount-deploy-host-post-switch-smoke
```

If the switch SSH dies during `reloading user units for root`, use the
Eternal Terminal window. Then:

```bash
just status
just logs
just host-logs -- --status
```

Do not reboot. Agents never reboot.

---

## 6. After the generation is live

### 6.1 Units and logs

```bash
just status
just logs
just host-logs
just host-logs -- --status
```

Guest `/root/justfile` is a symlink to `/etc/surmount/root-justfile`
after a switch (`just status`, `just logs`, `just btop`, `just scram`).

### 6.2 Optional public HTTPS end-to-end (not CI)

```bash
SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=https://services.surmount.systems just e2e-host
```

This is never a flake check. It needs a live host and
`SURMOUNT_E2E_BASE_URL`. Ban tests need `LAB_IP` unless `SKIP_BAN=1`.

Local hermetic e2e (not cutover):

```bash
just e2e
```

### 6.3 Static sites only (not a NixOS generation)

```bash
just deploy
```

That publishes apex/www from locked `github:SurmountSystems/site` plus
extra vhosts. It does not run `nixos-rebuild`.

### 6.4 Splora indexers (host-local)

Host-local `host-local-splora.nix` owns instance `enable`. Mainnet
indexer is off. testnet3, testnet4, mutinynet, and liquid are on in that
file. They do not index until cookie files exist:

```text
/run/surmount-secrets/splora/testnet3.cookie
/run/surmount-secrets/splora/testnet4.cookie
/run/surmount-secrets/splora/mutinynet.cookie
/run/surmount-secrets/splora/liquid.cookie
```

Path only. Never cookie bytes in git. JSON-RPC is loopback on Splora
default ports unless `daemonRpcAddr` in that same host-local file points
at a real node. Do not invent that peer in public docs.

### 6.5 MTA-STS

Host-local `mtaStsMode` may already be `enforce`. Live
`https://mta-sts.surmount.systems/.well-known/mta-sts.txt` stays
`testing` until a real switch. After switch, confirm that file shows
`mode: enforce`. The DNS TXT `_mta-sts.surmount.systems` id should
change when the policy changes so senders refetch. `surmount-dns-zone`
without `SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR` does not call live
Namecheap; bump that TXT with a live Namecheap path when that tool
supports it.

### 6.6 Certificate hostnames

When names on the Let's Encrypt certificate this host presents must
change:

```bash
just laptop-renew-cert -- --issue --directory production \
  --host-profile /home/hunter/.local/share/surmount/host-profile.toml \
  --namecheap-env /home/hunter/.local/share/surmount/issue-le-prod/namecheap.env \
  --install --restart-ui --target root@surmount-1 \
  --timeout-secs 3600
```

Pass `--hook` so extra-zone TXT uses `acme-dns-hook-namecheap-dispatch`.
`--live` does not add missing names. Namecheap ClientIp is laptop
egress. Do not mint a second certificate.

---

## 7. Git with a collaborating peer

- Work on git-flow `feature/<slug>` or `bugfix/<slug>`, not a pile on
  `main`.
- Agents never `git add`, never `git commit`, never GPG-sign, never
  `git push`.
- Operator signs on a real TTY and pushes.
- After that push, the agent opens the GitHub pull request for that
  branch. Base is `develop` if it exists, otherwise `main`. For
  `SurmountSystems/splora` the default branch is `surmount`; a push
  there is already on the default branch, so there is no pull request
  from that branch onto itself.
- Plan Approve: GitHub issue with the plan text (no secrets).
- Operator bug report: GitHub issue the same turn, screenshots attached.

---

## 8. Forbidden

- `BUILD_LOCAL=true` on `just check-remote` or as a way to rustc this
  laptop for product proof.
- Laptop `cargo test`, `cargo clippy`, `cargo build` for this repo's
  quality bar.
- Two live `just check-remote` processes.
- Real `just deploy-host` by an agent unless the operator overrides.
- Secrets, PEMs, cookie bytes, tokens, provisioned IPs in git, issues,
  or chat.
- Reboot.
- Starting Lake from an agent.
- Nice on `stalwart`, `management-ui`, `sshd`, `arti`, or networking.
- Treating `just deploy` (static sites) as a NixOS generation.
- Treating grok-oss `just test-remote` as this repo's gate. This repo's
  gate is `just check-remote`.

---

## 9. One-page command list

### Agent (through dry-run; operator does not run these to find errors)

```bash
ssh -o BatchMode=yes -o ConnectTimeout=8 nixbuilder@surmount-1 'echo nixbuilder_ok; hostname'

nix eval --raw '.#checks.x86_64-linux.module-eval-contract'
nix eval --raw '.#checks.x86_64-linux.deploy-secrets-contract'
nix eval --raw '.#checks.x86_64-linux.arti-module-contract'
nix eval --raw '.#checks.x86_64-linux.splora-package-contract'

just check-remote

just deploy-host -- --dry-run --target root@surmount-1 --host-local /home/hunter/.local/share/surmount/host-local
```

The agent reports each store path or the named failure, then the dry-run
exit 0, then asks for the switch.

### Operator (after the agent reports dry-run green)

```bash
just et
just deploy-host -- --target root@surmount-1 --host-local /home/hunter/.local/share/surmount/host-local
just status
```

Smoke already runs inside the deploy driver. Re-run only if the switch
SSH died:

```bash
ssh -- root@surmount-1 surmount-deploy-host-post-switch-smoke
```
