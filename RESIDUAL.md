# Residual (open work after Phases A-D foundation + review fix rounds)

**Last updated:** 2026-09-07. Intended production leaf is **20**
certificate hostnames on one Let's Encrypt PEM (`with_single_cert`): the
live 18 plus `cryptoquick.com` and `www.cryptoquick.com`. Live leaf
still has **18** names (CT). Validating A for cryptoquick apex/www
succeeds (AD true). Leftover parent DS key tag 2368 is gone. HTTPS
failure on those two names is certificate hostname mismatch, not
SERVFAIL. The old wait-for-SERVFAIL gate is closed as a live A-lookup
gate. SHA-1 parent DS digest type 1 remains standing DNSSEC quality debt
in operator-facts Monday leftover; it is not the HTTPS cause. Laptop
`--issue` after the tree lists the two names is operator-gated
(Namecheap custody). Do not invent leftover Namecheap clicks. Do not
MX-flip Baxter. Esplora Hosts stay off this leaf. Prior 2026-09-03. NIP-07 `/login` **Login failed: 500** on
HTTP/3 was Axum missing `ConnectInfo` on `POST /api/v1/auth/session`
(axum-h3 does not insert it; TCP HTTP/2 was JSON 401). Tree fix: optional
peer extractor + do not ban unspecified. Leftover: inject the real QUIC
peer into `ConnectInfo` so HTTP/3 bans and rate-limit keys are per-client
(today H3 middleware already falls back to 0.0.0.0 when the extension is
absent). NWC is not that 500; it is the `/mail` wallet store after login.
Live switch still operator-owned. Flake input `splora` is
`github:SurmountSystems/splora` on the `surmount` branch, locked rev
`9481e4cb87273aa99b0357be48503765beadb919` (previous lock
`343727487988ed0a764674ff21c0750465b9a3e8`). Imported
`nixosModules.splora` instance options now include `cookieFile`,
`daemonRpcAddr`, `jsonrpcImport`, `daemonDir` (null is remote JSON-RPC),
`publicHealth`, db cache default 24, and `httpSocketFile` default
`/run/splora/${name}.http.sock`. No QUIC on those Unix sockets. HTTP/2
and HTTP/3 stay on this-tree Axum. This tree **keeps**
`surmount.sploraIndexer` as the host-local single knob (one instance,
cookie path charset, sample stays off). The wrap maps those first-class
instance options. It does not mkForce ReadOnlyPaths and does not inject
`--jsonrpc-import` / `--public-health` through extraArgs. Eval of the
remote JSON-RPC wrap does not fight the imported module. Do not delete
the wrap. Indexer units stay off until private host-local sets that
option with a reachable JSON-RPC address and cookie path. grokOss is
unrelated to REST. Flake input grok-oss tracks
`github:SurmountSystems/grok-oss/remote-1` at PR 51 head
(`6edf5fda9507c9e3c9c9f5a67871ebb3ce6cca7d`; open PR, product tip
https://github.com/SurmountSystems/grok-oss/pull/51). Enable
`surmount.grokOss` still from private host-local only; public sample
stays default off. This slice did not bump grok-oss. Upstream crane omits
`.cargo/config.toml` from src and builds `--offline --locked`. Laptop
`cargo` still uses Menhera. This tree does not wrap crane. Menhera
in-sandbox fetch is already fixed upstream. The public sample host keeps
`surmount.sploraProxy.enable` default off, `surmount.sploraIndexer.enable`
default off, and does not set `services.splora.enable`. Private host-local
already enables `surmount.sploraProxy` with the five esplora Host maps.
Those Host maps and hypervisor UDP 443 are optional Axum edge, not
prerequisites of the mempool REST. Unix socket or one existing Host is
enough. Do not map REST onto the mail console Host.
`surmount.managementUi.http3Enable` stays default true. The live UI
process has `SURMOUNT_SPLORA_PROXY=1`. `/run/splora` exists from tmpfiles.
There are no indexer HTTP sockets and no `queue.sock`. Do not claim
Esplora is live in browsers. Do not claim the queue unit is live.
`just grok-oss` attaches as user grok (`runuser`) so MemoryMax can apply.
The grokOss module stays default off. Do not start Lake from this agent.
Do not invent MX / DMARC `p=reject` / Vaultwarden / ban / Q-AUTH-1 /
reboot.

**Open leftover (operator host-local, complete sentences):**

1. Private host-local still needs `surmount.sploraIndexer.enable = true`
   plus a reachable Bitcoin Core JSON-RPC address
   (`surmount.sploraIndexer.daemonRpcAddr`) and a cookie file path
   (`surmount.sploraIndexer.cookieFile`; never cookie bytes in git) so
   one indexer instance can start. A local bitcoind datadir on this
   guest is not required. Optional `publicHealth = true` passes
   `--public-health`. elementsd is not required unless a liquid instance
   is enabled. Do not start five indexers. Do not treat queue-only
   `services.splora.enable` as REST. Do not set this on the public sample
   host.
2. The operator still enables `surmount.grokOss` from private host-local,
   sets `package` to `pkgs.grok-oss`, and sets a MemoryMax budget string
   there. The flake pin is open PR 51 head on `remote-1`. That is grok-oss
   product leftover. It is not a mempool REST gate. That does not
   auto-start the TUI; start grok-oss in tmux as user grok after login.
   Default remains off.

**Highest value next (unblock):** private host-local JSON-RPC address,
cookie file path, and one indexer instance. Do not treat a local
bitcoind datadir as leftover. Do not wait on five esplora Let's Encrypt
names, `just check-remote` as a REST gate, hypervisor UDP 443, or
grokOss for the mempool API. Menhera in-sandbox is already fixed
upstream. Do not start Lake.

---

**Last updated:** 2026-08-25 (living mailbox map stays in operator-facts;
operator bins are `nix run .#...`.) Prior 2026-08-22 (living mailbox map:
`~/.agents/surmount-server/operator-facts.md`. Do not invent
MX flip / `p=reject` / VW. Prior 2026-08-20 (mail records on every mailbox domain, not
primary-only. Do **not** use `p=reject`. **Live public vs API:**
`surmount.systems` MX `10 mail.surmount.systems`, EmailType **MX**, SPF
`a:mail.surmount.systems -all`, dual DKIM, TLS-RPT, CAA; public `_dmarc`
at `1.1.1.1` **`p=quarantine`**; unsigned, no DS. `cryptoquick.com`
child (`+cd`): dual DKIM, TLS-RPT, CAA, SPF `a:mail.cryptoquick.com -all`,
MX `10 mail.cryptoquick.com`, DMARC **`p=quarantine`** (leave as-is).
That day's validating **SERVFAIL** was leftover parent DS **2368** alg 13 digest
type 1 SHA-1 (no child DNSKEY). **Live 2026-09-07:** leftover parent DS
key tag 2368 is gone. Validating A for cryptoquick apex/www succeeds
(AD true). Do **not** re-add 2368. The old wait-for-SERVFAIL gate is
closed. Intended leaf includes cryptoquick apex/www (20 names). Live
leaf still omits those two (18 names).
Extra MTA-STS wait. `baxterartworks.com` EmailType **FWD**; public MX
still eforward1-5 (do **not** claim MX flipped). `_dmarc.baxterartworks.com`
**intended** is **`p=quarantine`**. Do **not** restore getHosts `_dmarc`
to `p=none`. getHosts (11 records) has dual DKIM, TLS-RPT, CAA, SPF
`a:mail.surmount.systems` plus eforward include `-all`. Public `1.1.1.1`
2026-08-20: DKIM / `_dmarc` / TLS-RPT **NXDOMAIN**, CAA empty, old
eforward-only SPF `~all`, SOA serial **1787245654** not bumped. Public
`_dmarc` may stay NXDOMAIN while EmailType is FWD even when getHosts has
the TXT. Leftover `_acme-challenge` TXT still on getHosts. Extra domains
do **not** entirely lack dual DKIM / TLS-RPT / CAA. Extra-zone API
**worked** this slice (not still Invalid request IP). Remaining:
laptop `--issue` of the 20-name leaf after the tree lists cryptoquick
apex/www (Namecheap custody); SHA-1 parent DS digest type 1 as
operator-facts Monday leftover (not the HTTPS cause); Baxter Custom MX only if the operator asks
(`--live set-mx` fail-closed while FWD); public NS republish/lag for
Baxter getHosts vs served zone. No VW / ban / reboot. Dual-pin:
[AGENTS.md](AGENTS.md), [docs/DNS.md](docs/DNS.md),
[docs/SECURITY.md](docs/SECURITY.md),
[docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md).)
Prior same day: extra static vhost HTTPS **live** for six Namecheap
zones; that day's production leaf 17 certificate hostnames; contributor
self-serve mailbox password + optional NWC on `/mail`. Q-AUTH-1
unchanged.)
Prior 2026-08-19 (in-tree sshd hostKeys is ed25519 only when
hardening is on. Live host still advertises RSA until a `deploy-host`
switch. Do not wipe leftover RSA files this wave. Do not invent MX /
DMARC / VW / ban / Q-AUTH-1 / reboot / PQConnect as this change.
Prior same day: in-tree host paper trail: persistent
size-capped journald, sshd VERBOSE, structured `http_request` with peer
field and no Authorization/Cookie, `just host-logs`. **Live** on the
mail host (persistent `/var/log/journal`, SystemMaxUse 1G, unit journals
for mail/ui/sshd/fail2ban/nix-daemon). 2026-08-25 in-tree add: explicit
journald RateLimit knobs, `just host-logs -- --status` snapshot, Axum
WARN+ on stderr. RateLimit and stderr split wait on the next
`deploy-host` switch. qemu-guest-agent still host-local + that switch.)

Prior 2026-08-18 (HTTPS is durable: `surmount-management-ui`
ExecStart is the Nix store binary; `/run` 0640-bin drop-in **removed**.
`https://services.surmount.systems/health` **200**; apex **200**; IMAP
**993** Let's Encrypt production leaf covering `mail` + services +
apex + www + mta-sts. TLS key is **0600** `surmount-ui` (owner-only).
Stalwart uses copies under `secrets/mail/tls`. `surmount.remoteBuilder` **enabled** from private
host-local: `MemoryMax` + `Nice=19` on **nix-daemon.service** (real
rustc cgroup) as well as builder/user slices, `cpuQuota = auto` (not
95 percent of one CPU), `maxJobs` memory-safe (not a fake high advert),
niced `nix-daemon --stdio`, `nixbuilder` key-only. `surmount.lake` is
**default off** (niced + MemoryMax when host-local enables it). Do not
start Lake from an agent. SHC ticket 261
closed 2026-08-18. Agents never reboot. Do not open another ticket. Do
not invent MX / DMARC / VW / ban / Q-AUTH-1.
In-tree scaffold `memoryMax = "4G"` is not a published guest RAM size.
Scaffold `maxJobs = 8` is not a published core count.
Host-local may set a real cap privately. Niceness is not a memory cap.
The operator laptop is local. surmount-1 is the remote builder.
Laptop user Nix machines `max-jobs` matches live guest
`nix.settings.max-jobs` (guest-sourced; not laptop cores). Laptop
system `/etc/nix/machines` is still absent (agent `sudo -n` denied;
operator TTY commands in
`/home/hunter/.agents/reports/impl-untwist-inxi-machines-2026-08-18.md`).
User nix.conf already points at the user file. Measure the mail host
with `ssh surmount-1 inxi`; do not copy laptop `inxi` onto
`modules/remote-builder.nix`. Prior
same day: box recovered after hypervisor Stop then Start; UI HTTPS
was a temporary `/var/lib/surmount/surmount-management-ui-0640` plus a
`/run` drop-in (old store binary refused 0640; reboot would lose it).
Namecheap API stays laptop-only. Prior 2026-08-17: mail-plane TLS
apply driver shipped. **2026-08-18:** `point-stalwart-mail-tls` resolves
Stalwart 0.16.15 `query Certificate --json` from certificate hostnames
plus `id` (still accepts `filePath` if present). Laptop renew is a real
command: `just laptop-renew-cert -- --check|--live --directory production`
(issue only if due; stage/install/restart/prove otherwise). Laptop user
timer: `--install-timer --target ...` (without `--target` is BLOCKED).
Not a VPS ACME timer. Do not put a calendar
note in place of that path.)
Nix **ssh-ng** builder **live on the host** (MemoryMax generation
switched). Lake, if run at all, is **outside this flake**. Onion-Location + Alt-Svc
on the Axum edge; live Arti unit `surmount-arti-hidden-service` **active**;
same v3 hostname for apex, www, services, extra static Hosts, and
MTA-STS (`/_o/{host}` for non-console); Tor Browser verify
**BLOCKED**; operator HS identity backup still open; **B3 not fully
closed**; live VW unit still leftover). Prior 2026-08-14
(create-mailbox portal **shipped** on `/mail`;
review **0 open**; leftover-cookie / role None password is 403; NIP-98
query mismatch fail-open **closed**; reviewed tree **switched** on the
live host, `nixos-rebuild` 0; post-switch smoke 1, loopback `/health`
failed, cause unknown). Prior same day: primary hunter MailPlus copy
**done**; Vandelay import + JMAP export **done**;
TLS handshake logs journald-only; Safari
diagnosis = `tls_handshake_failed` while retrying the phone. Prior:
2026-08-12 earn-trust **live** Tracks 0/A/B/C before operator reboot;
report `.agents/reports/impl-earn-trust-wave.md`. Same day prior:
offline drivers + B4 live Nostr gate; LE production multi-name +
durable PEMs (laptop custody).
**Live (honest):** public **:443** is **surmount-manage** not Stalwart.
This laptop **2026-08-18 after hard cycle:** SSH up;
`https://services.surmount.systems/health` **200**; apex **200**;
IMAP/SMTPS Let's Encrypt YE1 including **mail**. SHC ticket 261
(closed 2026-08-18) said our RAM use at the tail end cannibalized the
hypervisor host and Proxmox automatically OOM-shutdown the guest as a
protection step. Console evidence 2026-08-18: Lean/Lake OOM killed
systemd and took inbound networking with it. Niceness is not a memory
cap. **Agents never reboot.** Do **not** open another ticket to re-argue
the Stopped badge. Cert is Let's Encrypt **production** (issuer YE2 as of later
measures; YE1 on the 2026-08-13 proof). Intended production leaf is
**20** certificate hostnames on one PEM. Live leaf as of 2026-09-07
(CT) is still **18** names:
`baxterartworks.com`, `www.baxterartworks.com`, `btcfur.com`,
`www.btcfur.com`, `exophiles.org`, `www.exophiles.org`,
`iantuckerstudios.com`, `www.iantuckerstudios.com`, `nostrfurs.com`,
`www.nostrfurs.com`, `yiffa.app`, `www.yiffa.app`, `surmount.systems`,
`www.surmount.systems`, `mail.surmount.systems`,
`services.surmount.systems`, `mta-sts.surmount.systems`,
`mail.cryptoquick.com`. Extra static
vhost HTTPS for those six Namecheap zones is **live** (HTTPS 200,
packaged site content, not console, not COMING SOON). Missing from the
live leaf: `cryptoquick.com` and `www.cryptoquick.com`. That hostname
mismatch is why cryptoquick HTTPS fails verify. Validating A succeeds
(AD true). Leftover parent DS key tag 2368 is gone.
Issued by **laptop DNS-01** (Namecheap; ClientIp = laptop egress); host
`SURMOUNT_ACME_ENABLE=0`; durable PEMs under
`/var/lib/surmount/secrets/tls/{cert,key}.pem`. Private host-local pins
`acme.enable = false` + those durable TLS paths + `authMode=nostr` +
`mtaStsMode=testing`. **Auth (B4):** anonymous services `/` **307** `/login`;
anonymous `/api/v1/domains` **401**; `/health` **200**. **Apex/www (live 2026-08-19):** packaged `SurmountSystems/site`
(flake input `1c84696` + `pkgs.surmount-public-site`; Host routing
on apex and www). `https://surmount.systems/` and
`https://www.surmount.systems/` **200** current GitHub tip copy (title
Surmount Systems, BIP 360 + P2MR, Grok OSS, contributors; nav/list
plus marks in `styles.css`). Not yellow UNDER CONSTRUCTION. COMING
SOON leftover **closed**. Services console stays on
`services.surmount.systems` (anon `/` **307** `/login`; `/health`
**200**). Report:
`/home/hunter/.agents/reports/impl-deploy-surmount-site-update-2026-08-19.md`. **DKIM engine:** both
File `DkimSignature` objects present on the **primary** domain, selectors
`stalwart` (Ed25519) + `stalwart-rsa` (RSA-4096), stage active (sign-ready;
**not** outbound signed mail; primary public MX is `mail.surmount.systems`).
Extra mailbox **DNS** is no longer "entirely missing": `cryptoquick.com`
child (`+cd`) has dual DKIM / TLS-RPT / CAA; `baxterartworks.com`
getHosts has dual DKIM / TLS-RPT / CAA; `_dmarc` **intended**
`p=quarantine` (public `_dmarc` may stay NXDOMAIN while EmailType is
FWD). Extra MTA-STS wait (not on this leaf). Extra-zone Namecheap
API **worked** this slice. Intended production leaf adds
`cryptoquick.com` / `www.cryptoquick.com` on the same PEM (20 names).
Live leaf still omits those two (18 names). Laptop `--issue` after the
tree lists them is operator-gated (Namecheap custody). **MTA-STS HTTPS:**
`https://mta-sts.surmount.systems/.well-known/mta-sts.txt` **200**
`mode: testing` (HTTP/1.1 and HTTP/2; domain-audit-style `curl --fail` OK);
policy `mx:` is product `mailHostname` (primary public MX is now
`mail.surmount.systems`; stay testing; do not enforce). **Recovery:** durable Domain B
`/var/lib/surmount/secrets/stalwart/recovery.env` mode **600**, unit
`EnvironmentFile=-.../recovery.env` (Q2). Report:
`.agents/reports/impl-earn-trust-wave.md`.
**Not** live VW; **mail is live** on the production leaf (993/465 YE1);
**primary MX flipped 2026-08-20** (`mail.surmount.systems`; Baxter MX still parked FWD; cryptoquick public MX already this host); mailbox `_dmarc` is
per-apex (primary public `p=quarantine`, cryptoquick `p=quarantine` leave as-is, Baxter intended `p=quarantine`; do **not** use `p=reject`; do **not** restore Baxter getHosts to `p=none`);
**not** live Tor Browser / onion-fetch proof; **not** operator
HS identity backup; **not** live ban drop; **Q-AUTH-1** still
open (durable session store, key-loss, first-operator bootstrap UX).
**This wave (done on the host):** `deploy-host` of current tree +
`surmount.remoteBuilder` with `MemoryMax`; store UI binary; `/run`
drop-in gone; health/apex **200**; IMAP Let's Encrypt; no Lake unit.
**Still open (not this module):** laptop system `/etc/nix/machines`
plus system `builders=` still need a local operator sudo TTY (laptop
is the Nix client; machines slots come from live guest, not laptop
inxi).
User nix.conf already has `builders = @` the user machines file. Lake stays **outside this flake**; do not
start it. Mail and critical units stay normal priority. Do **not**
invent MX / DMARC / VW / ban / Q-AUTH-1 / reboot / a new SHC ticket
to fill the queue. Law: [AGENTS.md](AGENTS.md)
**Mention is in scope; remote builder and niceness**;
[docs/OPS.md](docs/OPS.md) **Remote Nix builder**.
Reports: `/home/hunter/.agents/reports/impl-durable-https-remote-builder-2026-08-18.md`;
prior `/home/hunter/.agents/reports/impl-lake-memory-cap-2026-08-18.md`;
ticket thread `/home/hunter/.agents/reports/ticket-261-thread-lessons-2026-08-18.md`.
**Host package (btop, 2026-08-17; ET 2026-08-28):** `modules/btop.nix` adds
`pkgs.btop` when `surmount.enable`; laptop `just btop` uses Eternal
Terminal (`et -c btop`, live tty required, nested guest runs local
btop). **Not** on the live box until a `deploy-host` switch.
Hermetic `just test-btop-host`. Report:
`.agents/reports/impl-btop-ci-deploy-2026-08-17.md` (CI/deploy leftover).
**Host paper trail (2026-08-19, in-tree):** `surmount.logging` (default on)
persistent journal + size cap, sshd VERBOSE, nft ban-drop log prefix,
Arti `log_sensitive_information = false`, Stalwart/VW/UI stdout ->
journald, `nixbuilder` in `systemd-journal` when remoteBuilder is on.
`just host-logs` follows the journal over the same SSH target as
`just btop`. **Not** on the live box until a `deploy-host` switch.
No public log viewer. No secrets in logs. Scaffold `SystemMaxUse=1G`
is not a published guest disk size.
**Arti (honest 2026-08-17):** daemon unit active; hostname file present;
clearnet Onion-Location + Alt-Svc proven on HTTPS 307/200 for apex, www,
services. That is **not** B3 closed. **Status:** living residual after live
free-443 + production multi-name HTTPS (incl. mta-sts) + dual-sign register
+ MTA-STS testing + durable recovery + B4 + create-mailbox portal (review
0 open) + onion discovery headers + live Arti unit. **Post-reboot
proof PASS (2026-08-13).** Not operator acceptance of parked residual
(MX, VW, remaining Arti, ban, Q-AUTH-1). S6 values and live S7b unit remain
operator host residual. Hunter stays IMAP-only until an Administrator
pastes a real npub on `/mail` Grant console login (portal attach path
shipped 2026-08-20; no real npubs in tree). Named extra-person MailPlus
import from DS3018xs 2026-08-20. Living map:
`~/.agents/surmount-server/operator-facts.md`. Other unnamed MailPlus
uids deferred. feat:old-site-serve
deferred.

Reload SoT: [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md).

**Honesty pin:** local end-to-end green (`nix run .#e2e` / `just e2e`), including
self-signed certificate/key files and optional Tor with temp hidden-service
keys, is **not** public cutover, production `surmount-arti` ownership, or a live
kernel firewall drop. Host proof is `nix run .#e2e-host` / `just e2e-host` with
`SURMOUNT_E2E_HOST=1` on a real deploy. This environment has **no** staging/VPS
host (`SURMOUNT_E2E_*` unset; no active UI unit; `arti` not on PATH; `tor`
present only). Agent work below does **not** soft-elevate host rows.

**Production :80 (operator 2026-08-02):** assume TCP **port 80 is free** on the
NixOS box for the product **redirect-only** listener. That is **not** ACME
HTTP-01 on product :80 (still parked; Q-EDGE). Ops day-one:
[docs/OPS.md](docs/OPS.md); edge detail: [docs/EDGE_AND_TLS.md](docs/EDGE_AND_TLS.md).

---

## Validation SoT (developer / CI)

| Entry | What |
|-------|------|
| **`just check`** | Host CI-style bar: `fmt` (check-only, errors if dirty) then `clippy` (`-D warnings`) then `test` (all workspace cargo tests). Does **not** write format fixes. Does **not** hard-depend on Tor deep row or host e2e |
| **`just ci`** / **`just check-ci`** | Same as `nix build .#checks.<system>.ci` (full flake CI aggregate). GHA job display name is **`just ci`** (required-check footgun; see `.github/workflows/ci.yml`) |
| **`checks.*.ci`** | management-ui build + cargo fmt/clippy/test, **e2e-pure-test** (Rust pure helpers + host-gate contracts), one full module-eval, thin pure deploy/arti path contracts, arti-onion-package eval (features/passthru, no cargo build), nixfmt. **Does not** run host probes or optional Tor deep row. Workflow: [`.github/workflows/ci.yml`](.github/workflows/ci.yml) (pattern from grok-oss) |
| **`just fmt`** | Format **check** only (`cargo fmt --check` + flake nixfmt `--check`); use `just fmt-write` to apply |
| **`just test`** | Host `cargo test` only (fast loop) |
| **`just test-btop-host`** | Hermetic crate tests for `surmount-btop-host` (synthetic example.test; no live SSH). Also a GHA quality step. Live `just btop` is operator TTY only. |
| **`just clippy`** | Host `cargo clippy --all-targets -- -D warnings` |
| **`nix run .#e2e`** / **`just e2e`** | **Local comprehensive end-to-end** (SoT flake app; Rust `packages.e2e` / `surmount-e2e`). No NixOS, no root, no secrets in git: one hermetic `cargo test -p surmount-management-ui` plus named critical test anchors (health/TLS self-signed PEMs, https+local cleartext dual bind, rate-limit, ban/helper + remove_ban, surface audit 404/501, **Nostr auth off/gate/NIP-98 session**, onion unset residual, directory list + **account create auth-off/CSRF/lab-escape/503**, SSR, unauthorized stub) and `cargo test -p surmount-e2e --lib`. Not five independent cargo filters. Optional Tor publish+fetch when service-capable arti + Tor client present (`SURMOUNT_E2E_TOR=0` force-skip). Self-signed is correct for local HTTPS. |
| **`nix run .#e2e-host`** / **`just e2e-host`** | **Host end-to-end** (SoT flake app; Rust `packages.e2e-host` / `surmount-e2e-host`). **Never** a flake check. Requires `SURMOUNT_E2E_HOST=1` or exits **2**. Also requires `SURMOUNT_E2E_BASE_URL` (health FAIL if unset, not silent SKIP). Ban track requires `SURMOUNT_E2E_LAB_IP` membership in `surmount-ban4` unless `SURMOUNT_E2E_SKIP_BAN=1`. Summary `ban_drop=UNPROVEN` (set existence / membership is not live traffic drop). Probes: HTTPS health, MemoryDenyWriteExecute (no writable+executable memory), UI no CAP_NET_ADMIN (ambient+bounding), Arti unit, ban helper + set preflight. |
| **`just check-heavy`** | Optional mail-vm (not in ci) |

Heavy (not in ci): `mail-vm-test`, `mail-vps-eval` toplevel, `stalwart-mail` FOD.

CI note: only `module-eval-contract` runs full `nixosSystem`. Deploy-secrets and
arti-module flake checks are pure charset/path-shape only (no triple eval).
Module-eval is **not** a substitute for process end-to-end.

**E2E vs CI (product choice, not a law of nature):** Today `checks.*.ci` keeps
**reproducible tree checks** (including `e2e-pure-test` lib contracts) and
leaves the full hermetic anchor harness at `just e2e` / `nix run .#e2e`. Putting
**only** that hermetic path (anchors + cargo tests; `SURMOUNT_E2E_TOR=0` or no
Tor tools) into a flake check is a deliberate cost-vs-safety choice if we want
e2e always inside CI. **Tor deep row and host e2e stay out of CI either way** so
green CI never soft-elevates to "cutover done." Local temp Tor keys and
self-signed PEMs are not host ownership or public cutover.

---

## What shipped (honest)

### Foundation (Phases A-D) + review fix rounds 1-2

**Deploy secrets**

- Fail-loud activation + oneshot when `requireDeployMaterial`
- Strict host path charset (`modules/lib/host-paths.nix`); all shell uses
  `escapeShellArg`
- Per-path kinds: `requiredHostPaths` is `{ path; kind = file|directory; }[]`
  (no global kind, no weak `any`)
- Real module eval: `tests/module-eval.nix` (single full stack contract)

**Axum edge**

- In-process **rustls HTTPS** (TLS 1.3 lean, aws-lc-rs; hybrid PQ KEX when
  provider defaults enable it) loads host PEMs; `listenMode=https` happy path
- Fail-closed if PEMs missing/unreadable/wrong key mode; garbage PEM rejected
- `allowCleartextHttpsEscape` is emergency cleartext **override** (default
  false): when set it always binds cleartext under https, even if rustls is
  ready; bare https with PEMs (escape off) is the happy path at Nix eval
- Rate limit: loopback peers trust **X-Real-IP only** (XFF ignored for keys);
  max key cap; 429 + Retry-After integration test
- **Ban decision layer** (`ban.rs`): Allow / Whitelisted / RateLimited / Banned /
  BanCandidate; whitelist never banned; last-used touch on allowed whitelist
  traffic; memory backend + optional file state; nft command builder with
  hermetic `RecordingNftExec` (no root in CI). Enforcement default **off**
  (lean private). `accessControl` Nix opts + nft set names when enabled.
- **Least-privilege ban helper (scaffold):** binary `surmount-nft-ban-helper`
  (kernel firewall helper) + JSON protocol (`add_ban` / `remove_ban` / `ping`);
  product elevation is **socket-activated oneshot**
  (`surmount-nft-ban-helper.socket` + `@.service`) with CAP_NET_ADMIN on the
  **helper unit only**. UI uses `SURMOUNT_BAN_NFT_HELPER_SOCK` (Unix socket);
  keeps NoNewPrivileges and never gets CAP_NET_ADMIN. Child setcap spawn is
  **not** the host path (NNP blocks file-cap elevation). DryRun does not
  mutate firewall sets; Enforce apply-then-durable (no memory write if apply
  fails). Re-signal after crash-window treats "File exists" / already-present
  as apply success so app durable can catch up. `remove_ban` treats absent
  element as success (lab unban/cleanup). Hermetic tests + mock; no root/real
  kernel sets in CI.
  **Auth-failure BanCandidate stub shipped:** `BanGuard::signal_unauthorized` /
  `decide_ban_signal` + main request-context adapter; Off/DryRun/Enforce
  consistent; whitelist immune. **Surface audit + ban matrix locked:** routes
  do not auto-ban on 404/501; session exchange fail and bad presented NIP-98
  may signal once; missing cookie does not. Table dual-pinned in
  [docs/SECURITY.md](docs/SECURITY.md). Full Q-ACL surface list still open.
  **Not** live host drop.
- **Nostr auth foundation (2026-08-01; polish 2026-08-02):** rust-nostr NIP-98
  (kind 27235) + HMAC session cookie scaffold; `SURMOUNT_AUTH_MODE` off
  (default) / nostr; env allowlist + optional `SURMOUNT_NOSTR_ALLOWLIST_FILE`
  (env wins; empty fail-closed); challenge/session/logout/me + `/login`.
  **Not JS NDK.** UI honesty: auth-mode banners, `/login` when nostr.
  Create mailbox form is on `/mail` (Administrator only); `/accounts` still
  has no create form. Q-AUTH-1 residual: key-loss, durable session store,
  first-operator bootstrap UX (do not invent). nsec never on server.
- **Security headers + CSRF logout (2026-08-01):** management router baseline
  CSP (lean SSR; per-request nonce for `/login` NIP-07 script), nosniff,
  Referrer-Policy no-referrer, X-Frame-Options DENY + CSP frame-ancestors
  none. Double-submit CSRF cookie on session create; enforced on
  `POST /api/v1/auth/logout`. Session cookie HttpOnly + SameSite=Lax;
  Secure when HTTPS listen.
- TLS private key must not be group/world readable (`mode & 0o077 == 0`)
- Redirect helpers + host allowlist + IPv6 Host parse
- **Local cleartext full API** (`SURMOUNT_LOCAL_CLEARTEXT_LISTEN`): loopback
  only; concurrent with https primary; not the redirect-only :80 listener.
  Nix auto-derives when https UI + Arti enable + no explicit arti backend
  (public https -> `127.0.0.1:8090`; loopback https on 8090 -> `:8091`).
- **`surmount.web.enable` default false** (nginx not product edge); dual-run
  escape `web.enable = true` still wires nginx + ACME; module-eval asserts
  web-off / https-ui / dual-run-on paths (no secrets in tree)
- **Leptos SSR multi-page management console:** `pages.rs` renders
  `GET /`, `/domains`, `/accounts`, `/system`, `/mail`, `/login` via Leptos
  `view!` + `.to_html()` (ssr feature only; plain `<a href>` nav, no WASM).
  Shared `build_router` keeps rate-limit / ban / auth middleware. Honest accounts
  inventory (empty + `source: unavailable`; no fake `admin@`). Domains from
  config labeled as inventory, not Stalwart directory. Live Stalwart probe on
  overview/mail. Hermetic tests: SSR markers, DOGE palette, nav, no skeleton,
  accounts honesty, `/health`. No NPM. Crane: nixpkgs `rustPackages_1_95`
  (host channel nixos-26.05).
  **DOGE theme (2026-08-01):** pure 3-bit RGB eight-color palette only
  (`data-theme="doge"`, `color-scheme: only dark`); no grays / no light
  media queries. Spec SurmountSystems/specs `0001_DOGE.md` v1.0.0.
  **Directory trait + hermetic mock + live Stalwart client shipped
  (2026-08-01 mock; 2026-08-07 live):** `directory.rs` strategy on `AppState`
  (`UnavailableDirectory` default; `MockDirectory` via constructor or
  explicit `SURMOUNT_DIRECTORY=mock` only, never default-on;
  `StalwartDirectory` via `SURMOUNT_DIRECTORY=stalwart` + host Bearer token
  env/file, fail-closed misconfig, list fail-closed empty on HTTP error;
  management JMAP `x:Account/query` + `get`). API + SSR list through directory.
  **Mutations shipped (2026-08-07):** create/update via trait +
  `POST/PATCH /api/v1/accounts`; mock + `x:Account/set` wire-mock; auth gate
  (nostr or lab escape) + CSRF on cookie POSTs; no nsec.
  **Create mailbox form shipped (2026-08-14)** on `/mail` (Administrator;
  optional npub; User vs Administrator; map at
  `/var/lib/surmount/console/accounts.json`, env `SURMOUNT_CONSOLE_ACCOUNTS`).
  Review **0 open**. Leftover cookie / role None is **403** on password.
  NIP-98 query mismatch fail-open is **closed**. Map load fail-closed.
  Flock across load+edit+save. nsec not echoed. `ok: false` when
  `console_saved` is false. **Attach npub shipped (2026-08-20):** `/mail`
  Grant console login pastes `npub1...`, session CSRF, map write even when
  directory listing is unavailable. Hunter IMAP-only until an Administrator
  pastes a real npub. `/accounts` still has no create form. Q-AUTH-1 still
  open. MX still parked. No live VW / ban. Arti unit live; Tor verify
  and HS backup still residual (B3 not fully closed).
  **Mailbox password shipped (2026-08-14):** `POST /api/v1/accounts/password`
  + `/mail` form; lookup by email; Password credential on the mailbox Account
  (not API-token `AccountPassword`); live client POSTs `SURMOUNT_STALWART_URL/jmap`
  (0.16.15 `POST /api` is HTTP 404); works when listing is unavailable if host
  token + loopback URL exist; password never in JSON/logs/argv.
  **Structured request logging shipped:** onion-redacted path, no secret headers.
  Nix: `managementUi.directory` / `stalwartTokenPath` (default unavailable).

**Arti HS**

- Management-publish `arti.toml`: `[onion_services."<nickname>"]` +
  `proxy_ports` to management backend (TCP or `unix:` UDS); `storage.state_dir`
  = `onionServiceStateDir` (deploy secrets); cache under `stateDir`
- TCP backend: null `backendAddress` tracks UI bind when UI is http; when UI
  is **https** (escape off) and no explicit backend/UDS, module auto-points
  onion at the **local cleartext API** (`managementUi.localCleartextListen`
  or auto-derived loopback). Explicit `backendAddress` equal to primary https
  TCP still warns. UDS backend suppresses auto cleartext bind.
- Lean onion backend is **cleartext** HTTP (or UDS). No TLS-on-onion.
- `startDaemon` default **false** (enable installs config + status oneshot only)
- Complete lean path: `startDaemon=true` does **not** require
  `acceptIncompleteOnionConfig` (**no effect** this module version)
- Surmount package: `pkgs.artiOnionService` / `packages.*.arti-onion-service`
  is Surmount-owned Arti **2.5.1** source build + cargo feature
  `onion-service-service` (distinct from stock nixpkgs `pkgs.arti` 1.4.2).
  Module prefers it when `package` is null; capability via
  `passthru.surmountOnionServiceCapable` or explicit
  `packageIsOnionServiceCapable` (stock path still fail-closed)
- Daemon unit: package required; `ConditionPathIsDirectory` on HS state;
  restart burst capped; HS dir must be writable by `surmount-arti`
  (e.g. 0750 surmount-arti:surmount-arti; never auto-create identity dir)
- No private keys in tree; lean defaults (Stalwart admin/JMAP publish off;
  those flags reserved, no stanzas yet)
- **Onion IP collapse (documented limit):** lean TCP loopback rproxy means
  UI ConnectInfo peer is Arti (`127.0.0.1`); onion clients share one
  rate-limit/ban key unless a future PROXY / trusted-IP path lands. Do not
  invent X-Real-IP from Arti. Ban of 127.0.0.1 would deny cleartext+onion.
- **Onion-Location + Alt-Svc (2026-08-17; every public Host 2026-08-20):**
  `security_headers_middleware` emits both on mapped HTTPS 2xx/3xx (apex,
  `www.{apex}`, services, extra static Hosts, `mta-sts.{apex}`; same v3).
  Non-console Onion-Location uses `/_o/{clearnet-host}{path}`; onion-side
  rewrite serves that Host. `http://{onion}/` stays the services console.
  Mail unmapped. Mapping loaded once at process start from the existing
  onion surface plus extra static Hosts. No hot-reload; restart the unit
  after hostname file, map file, static vhost map, or env change. Optional
  env: `SURMOUNT_ONION_LOCATION_ENABLED`, `SURMOUNT_ONION_ALT_SVC_ENABLED`,
  `SURMOUNT_ONION_LOCATION_DISABLED_HOSTS`,
  `SURMOUNT_ONION_ALT_SVC_DISABLED_HOSTS`, `SURMOUNT_ONION_MAP_FILE`.
  `GET /api/v1/system` dumps `onion_discovery` (admin-gated when Nostr on).
  Local Arti cleartext bind does not emit discovery headers. Tor Browser
  purple pill still residual. Live extra-vhost and MTA-STS dual headers
  proven 2026-08-20 after `deploy-host` (same v3, `/_o/{host}`). Vaultwarden
  unit and `/vault` proxy still off (no Domain A vaultwarden-admin, no
  host-local enable fragment).
- **Live host (2026-08-17, private host-local only):** unit
  `surmount-arti-hidden-service` active; durable HS dir (not `/run`);
  hostname file present (v3 onion; do not paste the address in this public tree)
  (same v3 for apex, www, services). Host-local extras: `HOME=/var/lib/surmount/arti`;
  `surmount-ui` in group `surmount-arti` to read hostname; keystore 0700.
  Public module still does not set HOME / `port_info`. Arti does not write
  `hostname`; host wrote it from `arti hss onion-address`. Headers proven
  on HTTPS 307/200. Tor Browser purple pill **BLOCKED**.
- Residual: **live Tor network verification** (unit active != published;
  Tor Browser **BLOCKED**); operator offline backup of HS identity;
  systemd hardening parity in the **public** module after real `arti proxy`;
  Q-ARTI-*. Local e2e may publish with temp keys when arti is
  present; that is not operator-owned backup. Do **not** claim B3 fully
  closed.

**Tooling**

- `justfile` + flake `checks.ci` aggregate
- crane cargo test/clippy/fmt checks
- **Deploy automation (2026-08-08, offline; host-local import 2026-08-10):**
  host-local contract [docs/deploy-host-local.md](docs/deploy-host-local.md);
  operator driver `nix run .#surmount-deploy-host` (+ `just deploy-host`) with dry-run,
  lockout key check, refuse host-local into tracked `hosts/`/`secrets/`;
  **flake auto-import** of `./host-local` when present (known names +
  authorized_keys keyFiles; optional `default.nix`); rebuild uses
  `path:REMOTE#attr` so untracked host-local is visible; `/host-local/`
  gitignored; hermetic crate tests in `checks.*.ci`. Dual-pins: OPS, hygiene,
  SECRETS, sample host comments. **Not** CI remote switch; **not** live SSH
  cutover (needs operator target + working SSH + real keys).
- **Secrets custody bridge (2026-08-09/10, S0-S4 offline review-hardened,
  re-review 0 open):** three-domain living contract + attribute schema +
  material inventory + operator runbook in [docs/SECRETS.md](docs/SECRETS.md);
  `nix run .#secrets-install-host` (staging primary, optional secret-tool,
  refuse public inject, mode 0600, no value log); **S4** opt-in
  `nix run .#surmount-deploy-host -- --install-secrets` (default off; refuses missing
  staging/host-id; dry-run plans bridge; never logs values); hermetic
  crate tests in `checks.*.ci` + GHA quality only. **Not** libsecret
  Rust crate (S5 parked); **not** live keyring/host CI.
- **Host cutover automation pack A0 (2026-08-10, offline):** living gate
  checklist [docs/OPS.md](docs/OPS.md) + [docs/host-cutover-gates.txt](docs/host-cutover-gates.txt);
  `nix run .#surmount-host-material-inventory` (https-only / `--with-vaultwarden` /
  `--acme-path`; never logs values); lab DNS-01
  `nix run .#acme-dns-hook-lab` (temp zone; not production DNS);
  private host-local fragment emit (HTTPS/ACME/`requireDeployMaterial`/restic
  notes + S7b `surmount.vaultwarden.enable = true` only with VW profile);
  compose `nix run .#surmount-host-cutover` (default dry-run; `--live` + target for
  mutation; `--generate-material` for non-CA kinds; refuse in-tree staging and
  dash-shaped SSH targets); kind `vaultwarden-admin` on secrets-install +
  deploy allowlists; hermetic crate tests in `checks.*.ci` + just recipes +
  GHA quality step. **Scripts S6 install path and S7b enable path** after
  material exists. S6 **values** and live S7b unit remain **operator host**
  residual. Never live LE/SSH/keyring as CI green.

**Security posture docs (2026-08-08)**

- Ladder **B0-B7** documented as operator host work with proof gates (pointing
  at existing modules). Sample host still does **not** enable
  requireDeployMaterial / https / Arti / ban by default.
- **D1** TLS hybrid KEX offline: rustls workspace `prefer-post-quantum` +
  hermetic unit tests that default aws-lc-rs provider includes and prefers
  X25519MLKEM768; dual-pin EDGE/SECURITY/deploy-host-local/COMPACTION-PIN.
- **D1-host runbook + probe (offline ship 2026-08-08):** operator commands in
  [docs/deploy-host-local.md](docs/deploy-host-local.md) section 6;
  `nix run .#surmount-tls-hybrid` / `just check-tls-hybrid` (exit **2** BLOCKED
  when `SURMOUNT_E2E_BASE_URL` unset; hermetic
  crate tests with PATH-mock classical/MLKEM). Dual-pin
  OPS/EDGE/SECURITY/deploy-host-local/COMPACTION-PIN. **Not** live host hybrid
  until B1 + probe green on the real box. **Not** flake `checks.ci`.
- **Deploy/private-data/hybrid-probe/secrets/cutover CI:** GHA quality job runs
  hermetic crate tests in `checks.*.ci` (in addition to `--tree` scan).
  Hermetic only; never live `nix run .#surmount-tls-hybrid`, live keyring,
  live LE, or live SSH switch as green. Still not flake `checks.ci` remote
  switch; still not live cutover.
- **D2** PQConnect integration prep dual-pinned (consume after human sibling
  commit; 26.05 rebuild; keys never git; no premature flake path input).
  Product wire still human/operator-gated. Track C self-host DNS, B1+ host
  PEMs: still operator- or human-gated (see highest-value next).

---

## What remains

| Track | Residual |
|-------|----------|
| **Deploy on real host** | Driver + flake host-local auto-import + path: rebuild + **post-switch smoke** shipped offline (`nix run .#surmount-deploy-host-post-switch-smoke`; always after rebuild even if non-zero; hermetic fake systemctl/curl/ss self-test). Loopback `/health` is day-1 HTTP `:8090`; when the unit is public HTTPS edge (`SURMOUNT_LISTEN_MODE=https` and listen port 443) smoke curls `https://127.0.0.1:443/health` (`curl -sk`), not HTTP on :443. Smoke also checks TLS PEM path presence when https edge / `SURMOUNT_TLS_*` (durable H-PEM defaults; paths only), optional ACME enable note, and :80/:443 listen when https edge is configured (day-1 loopback skips PEM/listen). Operator still supplies target, private host-local material, working SSH, keys matching agent, and runs live switch. Activation hang residual (user@0 / "reloading user units for root") is **known ops residual**: prefer second SSH; verify generation + units via automatic smoke after recovery. No multi-host fleet tool. |
| **Public HTTPS host cutover (B1 / live W3)** | **Live free-443 + production multi-name HTTPS (2026-08-11; mta-sts on leaf 2026-08-12; mail on leaf 2026-08-18):** free-443 live; public **:443** is **surmount-manage**, **not** Stalwart; :8080 management + mail ports unchanged. `https://services.surmount.systems/health` **200** without `-k`. Cert LE **production**. Intended leaf is **20** names on one PEM. Live leaf as of 2026-09-07 is **18** names: six extra static zones apex+www (`baxterartworks.com`, `btcfur.com`, `exophiles.org`, `iantuckerstudios.com`, `nostrfurs.com`, `yiffa.app`) plus `surmount.systems`, `www.surmount.systems`, `mail.surmount.systems`, `services.surmount.systems`, `mta-sts.surmount.systems`, plus `mail.cryptoquick.com`. Missing: `cryptoquick.com` and `www.cryptoquick.com`. **Issue path:** laptop DNS-01 (Namecheap; SettleSeconds=120); host ACME **off**. Durable PEMs `/var/lib/surmount/secrets/tls/{cert,key}.pem`. Reports: `.agents/reports/impl-live-le-production-laptop.md`, `.agents/reports/impl-earn-trust-wave.md`, `.agents/reports/post-reboot-proof-2026-08-13.md`. **Post-reboot proof PASS (2026-08-13, real Blesta reboot):** units UI + Stalwart active; MTA-STS **testing**; both DkimSignature objects active; `recovery.env` mode 600; operator-only journal. **Later (2026-08-18):** IMAP/SMTPS Let's Encrypt YE1 including **mail**. **Later (2026-08-19):** apex/www packaged `SurmountSystems/site` (flake input `1c84696`; not UNDER CONSTRUCTION). **Later (2026-08-20):** extra static vhost HTTPS **live** for those six Namecheap zones (exclusive A matching this host; HTTPS 200; packaged site content; not console; not COMING SOON). Extra-vhost HTTPS is **not** remaining for those six. `baxterartworks.com` is also a local Stalwart Domain; public MX still parked. Static-site-only extras are **not** mail domains. **Not live in browsers as trusted HTTPS (2026-09-07):** `cryptoquick.com` and `www.cryptoquick.com` fail verify because the live 18-name leaf omits them. Validating A succeeds (AD true). Leftover parent DS key tag 2368 is gone. Host tree populated; insecure `-k` HTTP 200 titled Hunter Beast. Intended leaf is 20 names on one PEM. **Still residual (operator-gated):** laptop `--issue` after the tree lists the two missing names (Namecheap custody); SHA-1 parent DS digest type 1 in operator-facts Monday leftover (not the HTTPS cause; do not re-add 2368; do not invent leftover Namecheap clicks); firewall across switch proof; VW host enable + `/vault/` only with operator OK. **H4 parked.** Do **not** claim live VW. Mail **is** on the production leaf. Do **not** invent MX / DMARC / VW / reboot |
| **nginx module delete from tree** | Dual-run escape still ships (`web.enable = true`). Unused-detection checklist: [docs/OPS.md](docs/OPS.md). Delete module file only after operators no longer need it **and** explicit operator OK |
| **:80 redirect listener** | **Wired** in tree (flag + listen + allowlist; dual-run mutex; redirect-only; **same-host** HTTPS upgrade including apex/www so public users are not sent to the operator console). **Operator assumption (2026-08-02):** production port 80 is **free** for that bind. Host public proof still residual. **ACME HTTP-01 on product :80 parked** (Q-EDGE; free :80 does not invent ACME) |
| **requireDeployMaterial (B2)** | Module shipped; enable on host only after PEMs (and later Arti paths) exist so activation fails loud. Sample host leaves it commented. |
| **Arti live HS (B3)** | **Do not claim B3 fully closed.** Live Tor Browser / onion-fetch verify still residual (unit active != published). Operator offline backup of HS identity still open. Q-ARTI-2/3 still open. **Package currency shipped:** Surmount-owned Arti **2.5.1** source + `onion-service-service` (`artiOnionService`; not nixpkgs 1.4.2 lag). **Tree cleartext local backend for https+Arti auto-path shipped** (loopback API + onion target). Local temp-key publish (optional `just e2e` row) != operator backup. Do not invent Q-ARTI-2/3 answers. **Onion status product-real (2026-08-10):** when `artiHiddenService.enable`, management-ui derives hostname file + `SURMOUNT_ONION_HS_STATE_DIR` from `onionServiceStateDir` (walk nested hostname); structured status `configured` / `hostname_missing` / `not_provisioned` on SSR + `GET /api/v1/system`; residual names `surmount.artiHiddenService` (not demo env copy). Lab `SURMOUNT_ONION_URL` override only. **Onion-Location + Alt-Svc shipped (2026-08-17; every public Host 2026-08-20):** `security_headers_middleware` on mapped HTTPS 2xx/3xx; mapping loaded once at process start (restart after hostname/env/map change; no hot-reload). Same v3 for apex, www, services, extra static Hosts, and `mta-sts.{apex}`. Non-console Onion-Location uses `/_o/{clearnet-host}{path}`. Mail unmapped. Optional env: `SURMOUNT_ONION_LOCATION_ENABLED`, `SURMOUNT_ONION_ALT_SVC_ENABLED`, `SURMOUNT_ONION_LOCATION_DISABLED_HOSTS`, `SURMOUNT_ONION_ALT_SVC_DISABLED_HOSTS`, `SURMOUNT_ONION_MAP_FILE`. Dump on `GET /api/v1/system` `onion_discovery` (admin-gated when Nostr on). **Live host (2026-08-17, private host-local; extra/MTA-STS dual headers 2026-08-20):** unit `surmount-arti-hidden-service` active; durable HS dir (not `/run`); hostname file present (v3 onion; do not paste the address in this public tree); headers proven on HTTPS 307/200 for apex, www, services (2026-08-17) and extra/MTA-STS `/_o/{host}` (2026-08-20). Extra live wiring (host-local only): `HOME=/var/lib/surmount/arti`; `surmount-ui` in group `surmount-arti` to read hostname; keystore 0700. Public module still does not set HOME / `port_info`. Arti does not write `hostname`; host wrote it from `arti hss onion-address`. Tor Browser purple pill **BLOCKED**. Do **not** log full onion addresses in failure tails |
| **Nostr production auth (B4)** | **Live gated (2026-08-12):** public services `authMode=nostr`; anonymous `/` **307** `/login` (login HTML, not dashboard); anonymous `/api/v1/domains` **401**; `/health` **200**. Apex/www public page is packaged **SurmountSystems/site** (live 2026-08-19 GitHub tip `1c84696`; COMING SOON leftover closed). HTTP/2 `:authority` fallback already shipped. Domain B `session-secret` (EnvironmentFile) + `nostr-allowlist` installed (0600, `surmount-ui`; values never in git). Private host-local: `authMode=nostr` + paths + `publicBaseUrl` + ACME-off + durable TLS. Footgun guard live (public+off refuse). Report: `.agents/reports/impl-auth-live-b4-switch.md`. **Q-AUTH-1 still open** (durable session store, key-loss, first-operator bootstrap UX). Allowlisted operator browser login (NIP-07 / NIP-98) is operator-side proof, not a remaining host install. nsec never on server. |
| **Merciless ban product (B5)** | **First path + helper scaffold shipped:** Rust decide + memory/file; optional kernel firewall sync via Unix-socket helper oneshot or unsupported direct exec; Nix `accessControl` + sets; `nftHelper` requires `backend=nft` (fail-closed); UI no CAP_NET_ADMIN; EEXIST/already-present treated as apply ok for crash-window re-signal; **`remove_ban` / lab unban shipped** (hermetic delete-element + absent=ok). **Auth-failure BanCandidate stub shipped** + surface audit (404/501 do not auto-ban); bad NIP-98 / session exchange may call `signal_unauthorized` once (missing cookie does not). **Still residual:** Q-ACL-1..6 (which surfaces call the hook); live host helper + sets drop (`just e2e-host`); fail2ban SSH transitional (**B7** after B5 solid). **Not** live host drop / full unauthorized auto-ban product |
| **Mail earn-trust DNS/TLS (B6)** | **Primary `surmount.systems` live (2026-08-11/12, re-checked 2026-08-20):** Namecheap NS; operator clicked hosted **DNSSEC Status ON**; **live 2026-08-20** still no parent DS and no apex DNSKEY (Insecure, not SERVFAIL; waiting for Namecheap to publish); dual DKIM TXT `stalwart` + `stalwart-rsa`; DMARC **`p=quarantine`**; TLS-RPT; MTA-STS DNS + HTTPS testing; CAA Let's Encrypt. Domain B dual-sign PEMs on host. **Live DkimSignature (Track A, primary Domain):** both selectors stage active (sign-ready; **not** outbound signed mail). **Primary public MX flipped 2026-08-20** (dual-sign + PTR already green; operator Custom MX click; laptop `--live set-mx`). EmailType **MX**. Public and auth NS MX `10 mail.surmount.systems`. SPF `v=spf1 a:mail.surmount.systems -all`. DMARC **`p=quarantine`**. Laptop `issue-le-prod/namecheap.env` **`list` worked** (stored ClientIp accepted; file not rewritten). In-tree `list` prints **EmailType**. Leftover eforward MX gone. **`--live set-mx` still fails closed while EmailType is FWD** (not a public MX flip while Email Forwarding is on; extra domains; no EmailType-set API). `domain-audit` **FAIL**s registrar eforward on claimed mailbox domains. Local Stalwart RCPT for `hunter@surmount.systems` is **250**. **Public MX still eforward/FWD** on `baxterartworks.com` until asked. **MTA-STS HTTPS live (testing, primary only):** leaf covers `mta-sts.surmount.systems`; stay testing; do not enforce. **Extra mailbox domains (operator 2026-08-20; not primary-only):** this host already sends/receives locally for `cryptoquick.com` and `baxterartworks.com`. They do **not** entirely lack dual DKIM / TLS-RPT / CAA. **`cryptoquick.com`:** child (`dig @1.1.1.1 +cd`, SOA serial 1787271066) already has dual DKIM, TLS-RPT, CAA, SPF `v=spf1 a:mail.cryptoquick.com -all`, MX `10 mail.cryptoquick.com`, DMARC **`p=quarantine`** (leave as-is). Validating resolvers **SERVFAIL** on leftover parent **DS** key tag 2368, algorithm 13, digest type 1, **no child DNSKEY**. Operator UI this measure: Advanced DNS **DNSSEC Status off**. Do **not** re-add 2368. Same hosted **ON** after that DS is gone. Extra MTA-STS wait. Do **not** add apex/www to the production leaf. **`baxterartworks.com`:** EmailType still **FWD**; public MX eforward1-5 (do **not** claim MX flipped). getHosts (11 records) has dual DKIM (same `p=` as primary), TLS-RPT, CAA Let's Encrypt, SPF `v=spf1 a:mail.surmount.systems include:spf.efwd.registrar-servers.com -all`. `_dmarc` **intended** **`p=quarantine`**. Do **not** restore getHosts `_dmarc` to `p=none`. Public recursive `1.1.1.1` 2026-08-20: DKIM / `_dmarc` / TLS-RPT **NXDOMAIN** (may stay NXDOMAIN while EmailType is FWD even when getHosts has the TXT), CAA empty NOERROR, SPF still eforward-only `~all`, SOA serial **1787245654** not bumped. Leftover `_acme-challenge` TXT still on getHosts. Extra MTA-STS not published. `--live set-mx` **fails closed** while FWD. Extra-zone API **worked** this slice (not still Invalid request IP). Published Namecheap API has **no** DNSSEC/DS commands. Do **not** whitelist the VPS. **Still residual:** leftover cryptoquick parent DS (required for validating mailers to see those records); Baxter Custom MX only if the operator asks; public NS republish/lag for Baxter getHosts vs served zone; extra MTA-STS wait; `register-dkim --live --domain` as a host user who can read the token; rDNS/PTR; mail-tester. No reboot. Checklist: [docs/DNS.md](docs/DNS.md). |
| **D1 hybrid TLS proof** | Offline provider config shipped (prefer-post-quantum + hermetic unit tests). Host **runbook + probe script** shipped offline. **Live negotiation** residual after B1 on the real host. |
| **D2 PQConnect** | Integration prep docs dual-pinned. Sibling packaging; path flake input after **human** commit; rebuild on 26.05; keys host-only; Q-PQC-* open. Do not soft-elevate or wire flake early. |
| **Track C self-host DNS** | Optional; not required for B6. |
| **Admin Leptos SSR** | **Multi-page console + UI depth (2026-08-01):** SSR `/`, `/domains`, `/accounts`, `/system`, `/mail`, `/login` with DOGE chrome, status chips (UI / Stalwart / onion configured marker), inventory cards, definition-list system page, mail probe card. No skeleton branding. Accounts honest empty (`source: unavailable`) by default; domains config inventory only. **Directory trait + hermetic mock + live client shipped (2026-08-07):** `AppState.directory` (`unavailable` default; labeled `mock` fixture; live `stalwart` management JMAP with host token, never default-on; list fail-closed empty). **Account create/update mutations shipped (2026-08-07):** `Directory::create_account` / `update_account`; `POST /api/v1/accounts` + `PATCH /api/v1/accounts/{id}`; mock + live `x:Account/set` wire-mock; fail-closed when `auth_mode=off` unless lab `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED`; double-submit CSRF when session cookie present (same helper as logout). **Create mailbox form shipped (2026-08-14)** on `/mail` (Administrator only; optional npub; User vs Administrator; map at `/var/lib/surmount/console/accounts.json` / `SURMOUNT_CONSOLE_ACCOUNTS`). Review **0 open**. Leftover cookie / role None is **403** on password. NIP-98 query mismatch fail-open is **closed**. Map load fail-closed. Flock across RMW. nsec not echoed. `ok: false` when `console_saved` is false. Hunter IMAP-only until an Administrator attaches npub. `/accounts` still has no create form. **Mailbox password shipped (2026-08-14):** `/mail` form + `POST /api/v1/accounts/password`; lookup by email; Password credential on the mailbox Account (not API-token `AccountPassword`); works when listing is unavailable if host token + loopback URL exist. Docs scrub: README/STACK describe real console; Stalwart admin = bootstrap fallback. Directory research: [docs/research/stalwart-directory-api.md](docs/research/stalwart-directory-api.md). Day-one host order in [docs/OPS.md](docs/OPS.md). **Still residual:** hydrate islands only if clear SSR gap (parked); full Q-AUTH-1 product answers; **primary public MX flipped 2026-08-20** (`mail.surmount.systems`); Baxter MX parked FWD; JMAP proxy + webmail **beyond** thin 501 (501 honesty locked); **host cutover / hardware deploy still open for operator**; host must place Stalwart API token for live directory list and for mailbox password when listing stays unavailable |
| **Nostr E2E auth** | **Foundation shipped (2026-08-01):** `SURMOUNT_AUTH_MODE` off (default) / nostr; rust-nostr NIP-98 verify (kind 27235); env allowlist fail-closed when empty; HMAC session cookie scaffold; challenge/session/logout/me + `/login`. **Not JS NDK.** **Security headers + CSRF logout shipped (2026-08-01):** baseline CSP (nonce for login script) / nosniff / Referrer-Policy / frame denial on management router; double-submit CSRF on `POST /api/v1/auth/logout` (session cookie remains HttpOnly + SameSite=Lax; Secure when HTTPS). **CSRF on account mutation POSTs shipped (2026-08-07)** (cookie session requires `X-CSRF-Token` / body `csrf`). **Structured request logging shipped (2026-08-07):** method/path/status/latency; onion-redacted path; no Authorization/Cookie/bodies. **Q-AUTH-1 residual (do not invent):** key-loss recovery; durable server session store choice; first-operator bootstrap product UX. nsec never on server. Full webmail CSP still residual. |
| **JMAP proxy + v1 webmail** | **Thin 501 boundary shipped / locked:** `POST /api/v1/jmap` returns honest 501 (`error: jmap_proxy_not_implemented`), mail SSR documents residual, `/webmail` is 404 (no fake UI), surface audit no auto-ban on 501. Hermetic: `jmap_proxy_returns_honest_501_body` + `mail_page_documents_jmap_proxy_501_residual` + surface audit. **Beyond 501 (real authenticated proxy + v1 webmail UI) parked** offline-capable; do not invent. |
| **Operator OS secret store + deploy bridge** | **S0-S4 + A0 + V0/V1 + ladder P0-P7 + S8 + H2/H2b + H3 + H5/H6 + H7 offline shipped (2026-08-09/11):** three-domain contract (Domain C = human custody; cutover = A→B + host-local + DNS APIs); laptop is long-term custody SoT; VPS durable Domain B default `/var/lib/surmount/secrets` for PEMs + session/token/namecheap/VW (survives reboot); `/run/surmount-secrets` still allowlisted ephemeral. `secrets-install-host` (`namecheap-api`, optional `--client-ip` / `--require-namecheap`); namecheap hooks refuse non-owner-only env mode; **A0** `host-cutover` multi-step ladder + remote free-443; packaged `acme-dns-hook-namecheap` code for `dnsHookPath`; hermetic tests. **H2/H2b** durable PEM + ACME path defaults and parent modes; **H3** recovery unit `EnvironmentFile` option + strip path; **H5/H6** multi-name render + no silent prod + post-switch PEM/listen smoke; **H7** permanent Admin before ApiKey (compose/bootstrap). **This wave (offline):** recommended durable default `recoveryAdminEnvFile` = `/var/lib/surmount/secrets/stalwart/recovery.env` (`mail.nix` `mkDefault`; explicit `""` opts out; t34b/t34f); named `just laptop-renew-cert`. **Live host (2026-08-11):** free-443 applied; Domain B token path used for free-443; **production** multi-name PEMs on durable `/var/lib/surmount/secrets/tls/` (laptop DNS-01; host ACME off). **Not shipped / still residual:** S5 Rust/libsecret (**parked**); Namecheap VPS whitelist only if host ACME ever wanted (not this path); live free-443 / live LE as **CI** green (forbidden); durable recovery.env is **live** (Track C 2026-08-12); **post-reboot proof PASS (2026-08-13)**; **namecheap.env co-located under ACME `ReadWritePaths` parent** (default `.../acme/namecheap.env` next to account.json; UI can rewrite env without a separate path grant; prefer sibling leaf e.g. `.../secrets/dns/` under H4, not this slice); **install bridge always re-chmods material root / state prefix to 0755** even when operator set tighter modes (create-only or "widen only if lacking exec" residual); **H4** zero-cred-on-VPS redesign (parked). Living: [docs/SECRETS.md](docs/SECRETS.md), [docs/OPS.md](docs/OPS.md) ladder + gates, [docs/host-cutover-gates.txt](docs/host-cutover-gates.txt). |
| **Vaultwarden (domain C)** | **S7a shipped offline (2026-08-09):** module + UI; sample enable=false; pure eval + hermetic UI tests. **Do not rebuild S7a.** **S7b A0 offline shipped:** allowlisted kind `vaultwarden-admin` -> durable `/var/lib/surmount/secrets/vaultwarden/admin.env` (S8; `/run` alias still inventory-OK); cutover `--with-vaultwarden` inventory + private `surmount-vaultwarden-enable.nix` + restic path note + smoke recipes. **V4 offline shipped (2026-08-10):** human-store runbook in [docs/SECRETS.md](docs/SECRETS.md) section 5.1 (what goes in VW vs A vs B); explicit VW never feeds `nixos-rebuild`; optional export helper `nix run .#secrets-export-bw-to-staging` + hermetic crate tests (fixture + mock `bw`; not live VW; not activation). OPS pointer for G8-G10. **Phase B offline shipped (2026-08-11):** Axum reverse-proxy under `/vault/` in management-ui (`proxy_vaultwarden.rs`); Nix knobs `vaultwardenProxyEnable` / Prefix / Upstream (default **off**); hermetic tests green; docs EDGE + SECURITY dual-pin. Report: `.agents/reports/impl-phase-b-vw-axum-subpath.md`. **Not** live host VW unit; **not** public same-origin vault until operator enables proxy (public HTTPS edge has production LE `/health` 200; do not enable VW this wave). **Still residual (S7b host / live):** operator token value (or `--generate-material`), live switch, unit active, first VW admin UI account once, optional public/onion exposure (do not invent Q-ARTI); enable `vaultwardenProxyEnable` only with operator OK. **Export is optional residual comfort**, not a cutover blocker and never a boot dependency. **H0:** track `modules/vaultwarden.nix` when human commits (agents never `git add`). **Q-SEC-SM-*** open (do not invent Bitwarden Secrets Manager). Never secrets in git. |
| **Synology NAS / MailPlus + static HTML** | **Inventory done (2026-08-13).** AFP shares `MailPlus`, `mail`, `sites`, `surmount` mounted read-only. Report: `.agents/reports/nas-inventory-mail-site-2026-08-13.md`. Mail is **MailPlus** `@local/<uid>/<uid>/Maildir` (maildir-nested). The **mail** share is empty. **Uid map:** living map is `~/.agents/surmount-server/operator-facts.md`. Hunter stays **IMAP-only** until an Administrator attaches npub. Extra people are ordinary User mailboxes. Some leftover MailPlus accounts are hunter aliases. Same local-part on another domain is **not** an alias. Distinct MailPlus accounts are separate User mailboxes. MailPlus copy/import for a same-local-part extra-domain mailbox is **still not done**; do **not** fold it into the primary hunter uid. Do **not** import unnamed extra uids unless named. This GVFS AFP mount **cannot readdir non-empty `cur/`** (I/O error). **Copy path (2026-08-13):** `gio list` + reconstruct Maildir `:2,` flags + `rsync --files-from` (open-by-name). SMB still password-blocked; NAS SSH 22/2244 refused. Durable user unit `surmount-copy-1029` **finished** (Result=success, folders_ok=31, folders_fail=0, listed_total=535991). Dest: `/var/lib/surmount/import/maildir/1029/1029/Maildir`. `.All Mail` excluded and **absent**. Dest vs source gio **passed** (INBOX 88912, `.Sent` 7972, `.Archive` 179387, `.Surmount` 1685; dest not short). Only uid `1029` under import/maildir. Report: `.agents/reports/copy-1029-maildir-2026-08-13.md`. **Import done (2026-08-14).** Maildir->Vandelay archive: 32 mailboxes, 535961 emails. JMAP export **success** (unit `surmount-import-1029` Result=success, ExecMainStatus=0; export-only 01:28:50Z-13:14:39Z). JMAP totals **exact** vs dest: Inbox 88912, Sent Items 7972, Archive 179387, Surmount 1685, sum 535961. No All Mail folder. Dest `.All Mail` still **absent**. Only uid `1029` under import/maildir. Temporary Http/Jmap push limits **restored** (authenticated rate 1000/60s; Jmap concurrent 4/4, uploadCount 1000, uploadQuota 50e6). Loopback session rewriter **stopped** (:18080 closed). Live `stalwart-cli 1.0.12` has **no `import` subcommand**. **Live PATH:** `vandelay` **1.0.7** + `surmount-mail-import-maildir`. Snapshot `/var/lib/surmount/backup/stalwart-mail-pre-1029-2026-08-13` (do not delete). Dest and archive **kept**. Primary public MX flipped 2026-08-20 (`mail.surmount.systems`). Report: `.agents/reports/import-hunter-1029-2026-08-13.md`. Old company page is **`surmount/Site`** (static HTML). `sites/` is other vhosts; `lunarlupine` is Grav PHP (do not serve as product). **DS3018xs import (2026-08-19/20):** extra people as ordinary **User** only (no console Administrator). Primary hunter was not re-imported from DS3018xs. Operator still sets mailbox passwords on `/mail`. Extra-domain same-local-part User principal **created** 2026-08-22. MailPlus copy/import for that mailbox is **not done**. Do not import unnamed extra uids unless named. Living map: `~/.agents/surmount-server/operator-facts.md`. Two office DiskStations: canonical host ids **DS1513** (5-bay) and **DS3018xs** (6-bay). Discover + AFP mount + uid copy scripts shipped (`just diskstation-discover`, `just diskstation-afp-mount -- --host DS1513`, `just mailplus-copy-uid -- --host DS3018xs <uid>`). Copy `--host` matches IPv4-named GVFS via laptop-private hint (never print IPv4). **AFP mount accepts operator IPv4 at runtime** (`--afp-host` / `--uri` / laptop-private hint `~/.local/share/surmount/diskstation-afp-hosts`, never git). `--host` is the Secret Service label. **Nautilus/gio remember shipped:** `secrets-prompt` stores GNOME NetworkPassword (`protocol=afp`) beside `synology-afp`; mount fills it if missing. DSM rename is optional. mDNS is optional. Last-week path is gio by operator IPv4. Do not publish office LAN IPv4 in this tree. Secret kind `synology-afp` is laptop-only (install-host refuses); one item per NAS. Other MailPlus uids still deferred. **feat:old-site-serve** (old NAS `surmount/Site` as product serve) **deferred**. Public site document root shipped; live files at `/var/lib/surmount/public-site` from sibling site repo (host deploy of this binary still needed). NAS stays read-only. |
| **sshd ed25519-only host keys** | **In tree (2026-08-19):** `modules/hardening.nix` sets `services.openssh.hostKeys` to one ed25519 key at `/etc/ssh/ssh_host_ed25519_key`. Hermetic `just test-sshd-hostkeys-eval` (t43). Password-off / no kbd-interactive unchanged. **Live leftover:** the running host still advertises RSA until a `deploy-host` switch. Existing RSA files on disk are not deleted by dropping rsa from `hostKeys`. Do not deploy this wave. Do not wipe live keys. Do not reboot. **PQ honesty:** this is classical SSH hygiene. Ed25519 is not PQ. OpenSSH host keys are not PQ. PQConnect is not this. |
| **LUKS live / PQConnect / UDS Stalwart / search / multi-host** | As before. |

### Remaining mailbox import (2026-08-21)

Living map: `~/.agents/surmount-server/operator-facts.md`. Same local-part
on another domain is not an alias. Distinct MailPlus accounts are separate
User mailboxes. Password on services `/mail`. Do **not** invent extra-domain
MX flip, DMARC `p=reject`, or Vaultwarden for this slice.

### Remaining operator UI (2026-08-20, extra mail domains)

Laptop Namecheap custody. Not the VPS. No IPs or tokens here.
**Primary `surmount.systems` public MX is live** (`10 mail.surmount.systems`;
EmailType MX; SPF `a:mail.surmount.systems -all`; public `_dmarc`
`p=quarantine`). Extra-zone API **worked** this slice (Baxter getHosts
11 records). `cryptoquick.com` public MX is this host (Custom MX).
`baxterartworks.com` MX still parked FWD. No reboot.

**Primary Custom MX leftover is closed.** `--live set-mx` still is **not**
a public MX flip while Email Forwarding is on (EmailType FWD fails
closed). While Email Forwarding is on, Namecheap publishes eforward MX
and drops custom MX from `getHosts`.

1. **Cryptoquick HTTPS is a certificate hostname mismatch, not SERVFAIL.**
   Live 2026-09-07: leftover parent DS key tag 2368 is gone. Validating
   A for `cryptoquick.com` and `www.cryptoquick.com` succeeds (AD true).
   The old wait-for-SERVFAIL gate is closed. Live production leaf still
   has **18** names and still omits those two. Intended leaf is **20**
   names on one PEM (`with_single_cert`). After the tree lists the two
   names, laptop `--issue` is operator-gated because Namecheap
   credentials and ClientIp are laptop custody. SHA-1 parent DS digest
   type 1 remains standing DNSSEC quality debt in operator-facts Monday
   leftover; it is not the HTTPS cause. Do **not** re-add 2368. Do
   **not** invent leftover Namecheap clicks. Path: [docs/DNS.md](docs/DNS.md)
   *Best DNSSEC we can actually run*.
2. **`baxterartworks.com` Custom MX only if the operator asks.**
   EmailType is still **FWD**; `--live set-mx` fail-closed. Public MX
   still eforward1-5. Do **not** claim MX flipped.
3. **Public NS republish/lag** for Baxter getHosts vs served zone.
   `_dmarc.baxterartworks.com` **intended** is **`p=quarantine`**. Do
   **not** restore getHosts `_dmarc` to `p=none`. getHosts already has
   dual DKIM, TLS-RPT, CAA, and the new SPF. `1.1.1.1` 2026-08-20 still
   NXDOMAIN for DKIM/`_dmarc`/TLS-RPT (public `_dmarc` may stay NXDOMAIN
   while EmailType is FWD even when getHosts has the TXT), empty CAA,
   old eforward-only SPF, SOA serial **1787245654**. Leftover
   `_acme-challenge` TXT still on getHosts. Extra MTA-STS wait.

---

## Highest-value next (with acceptance criteria)

Shipped in tree (do **not** re-list as next work): in-process rustls HTTPS,
Arti HS management-publish + package overlay, **https+Arti auto local
cleartext backend**, **Onion-Location + Alt-Svc on Axum HTTPS** (2026-08-17),
nginx default-off / dual-run escape, :80 redirect-only
bind, **P1 Axum product edge pin** (docs + free Stalwart :443 apply template
under `nix/stalwart/` / `/etc/surmount/stalwart/`), ban decision first path +
helper `add_ban`/`remove_ban`/`ping`, Leptos SSR **multi-page management
console** (not skeleton), surface audit (no auto-ban on 404/501), **live
Stalwart directory client** (explicit opt-in), **JMAP thin 501 honesty
contracts**, **account create/update mutations** (auth + CSRF), **create-mailbox
portal on `/mail`** (review 0 open; leftover-cookie 403; NIP-98 query
fail-open closed), **structured request logging**, hermetic e2e-host
pure helpers. Details under **What shipped**.

**Ranking (2026-08-11) after live free-443 + production multi-name HTTPS +
durable PEMs (laptop custody) + offline harden H2-H7:**

**Host durability bottleneck (narrowed):** production LE multi-name cert
(intended **20** certificate hostnames on one PEM; live **18** as of
2026-09-07: six extra static zones apex+www plus
surmount apex/www/mail/services/mta-sts plus `mail.cryptoquick.com`;
extra-vhost HTTPS **live** for those six; cryptoquick apex/www **not**
on the live leaf), durable Domain B PEMs,
dual-sign engine register, MTA-STS **testing**
policy HTTPS, durable `recovery.env`, and B4 Nostr gate are **live**
(2026-08-12; mail on leaf later 2026-08-18; extras on leaf 2026-08-20).
Apex is packaged
**SurmountSystems/site** (not UNDER CONSTRUCTION). **Post-reboot proof PASS
(2026-08-13).** **Lockdown prove PASS (2026-08-13):** public 22/80/443 are
sshd + Axum; Stalwart :8080 loopback; auth nostr; secrets mode 600; no
extra public listeners. Report:
`.agents/reports/vps-lockdown-away-2026-08-13.md`. Host ACME stays **off**.
Remaining **host/operator now:**
Primary hunter mailbox principal exists and is **no longer empty**. Living
map: `~/.agents/surmount-server/operator-facts.md`. **Vandelay 1.0.7** is on the
live host PATH (wrapper uses `vandelay import maildir` then
`vandelay export`, not `stalwart-cli import`). Primary hunter copy + import
**finished**. Primary public MX flipped 2026-08-20 (`mail.surmount.systems`;
EmailType MX). Public site is packaged
`pkgs.surmount-public-site` from flake input `github:SurmountSystems/site`
(live 2026-08-19 tip `1c84696`; not UNDER CONSTRUCTION). Extra people imported from DS3018xs (ordinary User; mailbox passwords still operator). Extra-domain same-local-part User principal created 2026-08-22. MailPlus copy/import for that mailbox is **still not done**; do not import into the primary hunter uid. Other unnamed uids deferred. Extra-domain Custom MX only if asked, leftover SHA-1 parent DS digest type 1 in operator-facts Monday leftover (not the HTTPS cause; leftover DS 2368 is gone), optional S7b + `/vault/` with
operator OK. Do **not** claim live VW.

| Bucket | Highest-value next for parallel agents |
|--------|----------------------------------------|
| **Ship (offline)** | Earn-trust offline drivers already shipped. Do not re-ship register-dkim / laptop-renew-cert / packaged SurmountSystems/site (not UNDER CONSTRUCTION) / recoveryAdminEnvFile defaults. Do not invent Q-AUTH-1 / Q-PQC / Q-HOST / Q-SEC-SM; S5 parked; do not rebuild S7a |
| **Park (offline)** | **H4** zero-cred-on-VPS DNS-01 redesign; full JMAP proxy + v1 webmail beyond 501; hydrate islands without clear SSR gap; Track C self-host DNS; D2 PQConnect product wire until human sibling commit; inventing any Q-*; S5 Rust/libsecret; live free-443 / live LE as CI green (forbidden); DMARC `p=reject`; extra-domain MX flip; import over AFP GVFS |
| **Host-gated (highest value now)** | **This wave (host done):** HTTPS store binary + remoteBuilder MemoryMax on nix-daemon.service; maxJobs memory-safe; Lake unit absent. Leftover: laptop system `/etc/nix/machines` only if a daemon-only Nix path is used later. Public site packaged from `github:SurmountSystems/site` (apex/www; live 2026-08-19 tip `1c84696`). Primary hunter MailPlus copy + import **done**. Extra people imported as ordinary User (MX not flipped). Hunter aliases imported into hunter, not separate mailboxes. Extra-domain same-local-part is **not** those aliases. User principal **created** 2026-08-22. MailPlus copy/import for that mailbox is **not done**. Living map: `~/.agents/surmount-server/operator-facts.md`. Mailbox passwords still operator on `/mail`. Dual-NAS AFP scripts shipped (DS1513 / DS3018xs; `--afp-host` IPv4 at runtime for reach only; NetworkPassword `server` is host id / `<id>.local`, never IPv4; paste wipe after store; live Secret Service remount later; DSM rename optional). Remaining unnamed MailPlus uids deferred (not 1028/1035). Extra mail-domain DNS is **open** under B6 (live 2026-09-07 leftover parent DS key tag 2368 is gone; validating A for cryptoquick apex/www succeeds; intended leaf is 20 names; live leaf still omits cryptoquick apex/www; laptop `--issue` after the tree lists those names is operator-gated; SHA-1 parent DS digest type 1 remains operator-facts Monday leftover and is not the HTTPS cause; do not re-add 2368; do not invent leftover Namecheap clicks; Baxter getHosts already has dual DKIM / TLS-RPT / CAA; `_dmarc.baxterartworks.com` **intended** `p=quarantine`; public `_dmarc` may stay NXDOMAIN while EmailType is FWD; do **not** restore getHosts `_dmarc` to `p=none`; Baxter Custom MX only if asked). Extra-zone API **worked** this slice. `domain-audit` extra mailbox checks landed separately. Primary public MX is `mail.surmount.systems` (Custom MX leftover closed). Baxter public MX stays parked FWD. Also: rDNS when `shc-api` staged (O2); S7b / `/vault/` only with operator OK; D1-host hybrid; B2; B3 Arti; B5. Q-AUTH-1 product answers stay open. Do **not** invent extra-domain MX flip / DMARC `p=reject` / VW / ban / Q-AUTH-1 / reboot / a new SHC ticket as extra tracks. |
| **Operator handoff** | Stage/commit as needed so flake `just check-ci` is green (agents never stage); track `modules/vaultwarden.nix` when committing (H0). Host `just check` still sees the working tree. Human owns sibling `pqconnect` flake commit before D2 wire-up |

**Shipped offline (2026-08-08, Track D):** D1-host runbook (deploy-host-local
section 6) + `nix run .#surmount-tls-hybrid` (BLOCKED exit 2 when BASE_URL unset)
+ hermetic crate tests (PATH-mock classical exit 1 /
MLKEM exit 0) + just recipes; GHA quality step for hybrid self-test only;
dual-pin OPS/EDGE/SECURITY/deploy-host-local/COMPACTION-PIN; D2 integration
prep in `docs/research/pqconnect-local-packaging.md`. **Not** live host hybrid,
flake path input to uncommitted sibling, real switch, PEMs, Arti publish, or
ban drop.

**Shipped offline (2026-08-08, residual pass):** D1 `prefer-post-quantum` +
hermetic unit tests for aws-lc-rs hybrid group include/prefer-first; GHA runs
crate tests for private-data + deploy-host; just
recipes; COMPACTION-PIN / EDGE / SECURITY / deploy-host-local dual-pins.
**Not** host hybrid negotiation, real switch, PEMs, Arti publish, or ban drop.

**Shipped offline (2026-08-08, Track A):** host-local contract doc;
`nix run .#surmount-deploy-host` + hermetic crate tests; dual-pins
OPS/hygiene/SECRETS/SECURITY/EDGE; sample host comment pointers; security
ladder B0-B7 + D1 honesty docs; activation hang workaround documented.

**Shipped offline prior (2026-08-07):** account create/update mutations
(directory trait + mock + Stalwart `x:Account/set` wire-mock; auth gate + lab
escape; CSRF on cookie POSTs); structured `http_request` logging with onion
redaction (no secret paths/tokens/nsec). **2026-08-13:** TLS handshake /
accept failures emit `tls_handshake_failed` at warn (peer, SNI if known,
short error; no PEM). `http_request` also records Host and User-Agent
(truncated). Logs stay stdout -> journald for `surmount-management-ui`.
No HTTP log dump, no world-readable `/var/log` / `/var/lib/surmount` access
file. Safari / iPhone diagnosis: `journalctl -u surmount-management-ui -g
tls_handshake_failed` while retrying the phone. **Mutation e2e hermetic anchors** in
`HERMETIC_ANCHORS`: `account_create_auth_off_fail_closed_without_lab_escape`,
`account_create_cookie_auth_requires_csrf` (also covers PATCH+CSRF),
`account_create_lab_escape_mock_succeeds`,
`account_create_unavailable_directory_service_unavailable`. Local green !=
public cutover / host Arti / live ban drop.

### 1. Public HTTPS host cutover + MemoryDenyWriteExecute (operator)

Tree defaults already support product https UI with web off. **Host proof** is
still residual (`just e2e-host`). Do **not** claim live public cutover or host
MemoryDenyWriteExecute (no writable+executable memory) done from eval or
local e2e alone. Do **not** call that hardening "unsafe" without a failed run.

Acceptance (host; automate via
`SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=... SURMOUNT_E2E_LAB_IP=... just e2e-host`,
or `SKIP_BAN=1` when omitting ban track):

- [x] Public :443 serves **surmount-manage** after free-443 live (2026-08-11);
      Stalwart not on :443. **Still residual:** reboot/switch without host-local
      ACME-off fragment; firewall durable across switch
- [ ] Free production :80 binds product **redirect-only** listener when enabled
      (operator free-:80 assumption; not ACME-on-product-:80); live :80 was
      management-ui HTTP redirect after free-443 (confirm durable after switch)
- [x] Certificate and key files on host only under durable Domain B
      `/var/lib/surmount/secrets/tls/` (cert 0640 `surmount-ui:surmount-tls`,
      key 0600 `surmount-ui` owner-only; never in git; 2026-08-11 laptop
      production issue + secrets-install; 2026-08-27 key 0600).
      **Still residual:** host-local
      ACME-off so next switch does not depend only on runtime drop-in
- [ ] `systemctl show surmount-management-ui -p MemoryDenyWriteExecute` => yes
- [x] TLS health: `https://services.surmount.systems/health` **200** without
      `-k`; LE **production**. Live leaf as of 2026-09-07 is **18** names
      (six extra static zones apex+www plus surmount
      apex/www/mail/services/mta-sts plus `mail.cryptoquick.com`). Extra-vhost
      HTTPS live for those six. Intended leaf is **20** names on one PEM.
      Cryptoquick apex/www are **not** on the live leaf. HTTPS on those two
      names fails at certificate hostname mismatch. Validating A succeeds
      (AD true). Leftover parent DS key tag 2368 is gone. Reboot-safe proof still open
- [ ] nginx not product edge when web off
- [x] Cert **renew** laptop path: `just laptop-renew-cert -- --check|--live
      --directory production` (issue only if due; stages tls-cert/tls-key,
      installs cert 0640 / key 0600, restarts UI, proves health + IMAP).
      Laptop user timer
      `--install-timer --target ...` (without --target is BLOCKED). Host
      ACME stays off. ACME-on-Axum-:80 (HTTP-01)
      still parked; UI still needs a restart after PEM replace (hot-reload
      residual)
- [ ] Optional: remove `modules/web.nix` only after dual-run unused

### 2. Arti live Tor verify (operator)

Module + package overlay shipped. Local optional Tor row (`just e2e`) may
publish with **temp keys** when arti + client exist; that does **not** close
host residual.

Still residual (host):

- [ ] Live Tor network verification (**unit active != published**; Tor
      Browser purple pill **BLOCKED** 2026-08-17; clearnet Onion-Location /
      Alt-Svc is not an Alt-Svc upgrade proof)
- [x] Durable HS dir on host, owned/writable by `surmount-arti` (not `/run`);
      identity generated on first start; never in git
- [ ] Operator offline backup of that HS identity
- [x] Cleartext local backend when managementUi is https-only (tree:
      auto loopback cleartext API + Arti backend)
- [x] Onion-Location + Alt-Svc on mapped HTTPS 2xx/3xx; `onion_discovery`
      on `GET /api/v1/system`; no hot-reload (restart after hostname/env/map)
- [ ] systemd hardening parity in the **public** module after real `arti proxy`
      (host-local `HOME=/var/lib/surmount/arti` only)
- [ ] Q-ARTI-2 / Q-ARTI-3 not invented; admin/JMAP onion stanzas still off
- [x] Surface onion URL to operator (hostname file + SSR + system dump +
      discovery headers). Do not log onion in failure tails

### 3. Privileged ban helper (scaffold shipped; host residual)

Tree acceptance (hermetic; covered by `just e2e`):

- [x] `surmount-nft-ban-helper` binary + narrow `add_ban`/`remove_ban`/`ping`
- [x] UI Unix-socket client `SURMOUNT_BAN_NFT_HELPER_SOCK`; no CAP_NET_ADMIN on UI
- [x] Socket-activated oneshot unit holds CAP_NET_ADMIN/RAW (not child setcap;
      NNP-safe). Mutually exclusive with `nftExec`
- [x] DryRun does not mutate kernel firewall sets; Enforce apply-first fail-closed
- [x] Hermetic tests (RecordingNftExec + fake script + Unix socket); no root in CI
- [x] Decision layer contracts preserved (Allow/Whitelist/RateLimited/Banned)
- [x] **Auth-failure BanCandidate stub shipped** (`decide_ban_signal` /
      `BanGuard::signal_unauthorized` + request-context hook; whitelist immune;
      Off/DryRun/Enforce). Not full Q-ACL surface list / Nostr login.
- [x] Lab `remove_ban` (delete-element argv; absent element = ok); e2e-host
      cleanup guidance points at helper CLI

Still residual (not claimed done; host `just e2e-host`):

- [ ] Live host: enable `backend=nft` + `enforcement=enforce` + `nftHelper` +
      sets; prove ban IP lands in `surmount-ban4` / `surmount-ban6` and is dropped
- [x] Crash-window recovery: apply ok then die before durable persist;
      re-signal treats already-present element as success (EEXIST /
      "File exists"); tiny residual if other race messages appear on host
- [ ] Q-ACL-1..6 policy answers (do not invent)
- [ ] Which HTTP/mail surfaces must call `signal_unauthorized` (Q-ACL-1);
      tree audit only: 404/501 do **not** auto-call the hook today
- [ ] Replace transitional fail2ban sshd when Rust path covers SSH

### 4. Admin UI depth / auth / mail UI (residual after multi-page console)

- [x] Multi-page SSR console: overview, domains, accounts, system, mail
      (DOGE; honest empty accounts; config domain inventory; live Stalwart probe)
- [x] Directory trait + hermetic mock (`source: unavailable` default;
      labeled `mock` fixture for tests / `SURMOUNT_DIRECTORY=mock` only)
- [x] Live Stalwart directory client (2026-08-07): `SURMOUNT_DIRECTORY=stalwart`
      + host token (`SURMOUNT_STALWART_TOKEN` / `TOKEN_FILE`); management JMAP
      `x:Account/query`+`get`; hermetic wire-mock + fail-closed empty on error;
      Nix `managementUi.directory` / `stalwartTokenPath` (default unavailable;
      no default-on fake accounts). Domains stay config inventory.
- [x] **Operator UX pass (2026-08-10):** Overview health-first dashboard +
      compact Attention (product language; no env primary residual for
      auth/directory/onion/vault on Overview). System holds technical residual.
      DOGE craft pass (type scale, chip semantics ok/fail/warn/neutral).
      Contract: `overview_operator_language_no_env_primary_residual`. Living:
      [docs/SEARCH_AND_UI.md](docs/SEARCH_AND_UI.md) Phase 2 + UX principles;
      follow-on backlog [`.agents/reports/ux-residual-follow-on.md`](.agents/reports/ux-residual-follow-on.md).
- [x] **Synology-style Overview package home (2026-08-10):** DSM-like tiles +
      health strip + compact alert chips; removed lede/Hostnames/Navigate/
      residual essays from home. Hostnames + enable paths on System. Same
      honesty contract (rewritten for tile labels Off/Ready/Missing, Off/Linked).
      Report: [`.agents/reports/impl-synology-dashboard.md`](.agents/reports/impl-synology-dashboard.md).
- [ ] Deeper UX residual (not this slice): design-system/token extract, a11y/
      mobile, auth bootstrap product UX when Q-AUTH-1 answers land, webmail
      when JMAP proxy, live host cutover polish when B1/B3 green. Ranked in
      ux-residual-follow-on report.
- [ ] Leptos hydrate islands only where a clear SSR gap needs them (none forced
      this pass; DOGE SSR primary; no NPM)
- [x] Nostr auth foundation (mode/allowlist/NIP-98/session cookie; rust-nostr);
      e2e hermetic anchors shipped for off/gate/NIP-98 session + surface audit
- [x] Create-mailbox portal on `/mail` (2026-08-14): optional npub;
      Administrator vs User; map `/var/lib/surmount/console/accounts.json`
      (`SURMOUNT_CONSOLE_ACCOUNTS`). Review **0 open**. Leftover cookie /
      role None password **403**; NIP-98 query mismatch fail-open **closed**;
      map load fail-closed; flock across RMW; nsec not echoed; `ok: false`
      when `console_saved` is false. Attach npub on `/mail` Grant console
      login shipped 2026-08-20 (bech32, CSRF, map write without listing).
      **Contributor self-password + NWC (2026-08-20):** User PATCH own
      mailbox password; Administrator still any (support). User `/mail` is
      self-serve + Evolution facts, not create. Optional NWC URI at
      Domain B `/var/lib/surmount/secrets/ui/nwc.json` (save/clear only;
      no live pay path). Hunter IMAP-only until Admin pastes a real npub.
      `/accounts` still has no create form.
- [ ] Q-AUTH-1 residual: key-loss, durable session store, first-operator bootstrap UX (do not invent)
- [x] JMAP thin 501 honesty + hermetic route contracts (2026-08-07): body code,
      no invented methodResponses, mail SSR residual copy, no `/webmail` product
- [ ] JMAP authenticated proxy + v1 webmail UI beyond 501 (parked depth)
- [ ] Hardware / host deploy next (operator; not invented here)

---

## Open operator Q-* blocking live deploy

| Id | Why |
|----|-----|
| **Q-HOST-1** / **Q-HOST-2** | Provider / LUKS day-one |
| **Q-EDGE-1** / **Q-CA-*** | Shared durable Let's Encrypt PEMs for mail + web is the **offline** product path (File Certificate + `surmount-tls`). Live leaf still last proven without `mail`; host apply still residual. Public CA is Let's Encrypt production. |
| **Q-ARTI-2** / **Q-ARTI-3** | Onion surfaces (lean defaults in module) |
| **Q-DEP-1** | Deploy-secrets tool long-term |
| **Q-ACL-*** | Full ban policy |

---

## Ranking while VPS is pending (offline vs host)

**Local green is never cutover.** Keep offline agent work and host-gated work
in separate buckets. Do not soft-elevate host rows from hermetic e2e.

### Offline / agentable without a VPS

Shipped or shippable on the developer machine / CI. Prove with `just e2e` /
`just check` / hermetic cargo. Examples:

| Slice | Status |
|-------|--------|
| Nostr auth foundation + hermetic e2e anchors (off/gate/NIP-98 session) | **Shipped** (Q-AUTH-1 product answers still open) |
| Directory trait + hermetic mock | **Shipped** |
| Live Stalwart directory client (JMAP + hermetic wire-mock + Nix opts) | **Shipped 2026-08-07** (default still unavailable; host token when enabling live; no cutover claim) |
| Create-mailbox portal on `/mail` + optional npub + two roles + map | **Shipped 2026-08-14** (review 0 open; leftover-cookie / role None password 403; NIP-98 query fail-open closed; map load fail-closed; flock RMW; nsec not echoed; `ok: false` when `console_saved` false). **Attach npub 2026-08-20:** Grant console login binds `npub1...` to an existing mailbox (session CSRF; listing optional). **Self-password + NWC 2026-08-20:** User sets own IMAP password after login; NWC URI save/clear off git; no Lightning node. Q-AUTH-1 still open. MX still parked. Hunter IMAP-only until Admin pastes a real npub |
| :80 redirect-only wiring + dual-run mutex | **Shipped** (host public bind still residual) |
| Production free-:80 docs pin (redirect-only; not ACME invent) | **Pinned 2026-08-02** |
| CSP / CSRF / session hardening beyond scaffold | Residual product when other agents touch auth/UI; not host-gated |
| JMAP thin 501 honesty + route contracts | **Shipped / locked 2026-08-07** (not full proxy) |
| JMAP authenticated proxy + v1 webmail UI beyond 501 | Residual offline-capable (**parked** depth; do not invent) |
| Hydrate islands | **Parked** until a clear SSR gap; DOGE SSR primary; no NPM |
| Optional local Tor deep row when arti + Tor client on PATH | Local only; != host `surmount-arti` ownership |
| Host-local contract + deploy driver + self-test | **Shipped 2026-08-08** (not host cutover; not flake remote switch) |
| GHA hermetic deploy + private-data + hybrid-probe + secrets-install self-tests | **Shipped 2026-08-08** (+ secrets gate 2026-08-09; quality job; not checks.ci; not live hybrid/keyring/SSH install) |
| Security ladder B0-B7 docs | **Shipped 2026-08-08** (host stages still gated) |
| D1 hybrid provider config + hermetic unit proof | **Shipped 2026-08-08** (`prefer-post-quantum`) |
| D1-host hybrid probe script + runbook | **Shipped offline 2026-08-08** (live negotiation still host residual after B1) |
| D2 PQConnect integration prep docs | **Shipped offline 2026-08-08** (no flake wire; product wire still gated) |
| Q-AUTH-1 / Q-ACL / Q-ARTI / Q-EDGE / Q-PQC answers | **Do not invent**; park |
| Track C self-host DNS / D2 PQConnect product wire-up | Parked until operator rank + human sibling commit |
| Untracked / dirty tree stage handoff | **Operator stage/commit** so flake `just check-ci` sees intended files (agents never stage). Host `just check` / `cargo test` use the working tree |

### Host-gated (when the VPS arrives)

Require real deploy + `SURMOUNT_E2E_HOST=1` / operator secrets. Agents without
a host **stop** at docs, tree wiring, and local e2e. Ladder detail:
[docs/deploy-host-local.md](docs/deploy-host-local.md) section 6.

1. **B0 + deploy driver on host:** private host-local (keys non-empty),
   `nix run .#surmount-deploy-host` or manual rsync + `nixos-rebuild switch --flake
   .#mail-vps`; smoke `systemctl is-active`
2. **B1 Public HTTPS cutover + `just e2e-host`** (PEMs, MemoryDenyWriteExecute,
   public :443; free :80 redirect-only bind when enabled)
3. **D1-host hybrid negotiation probe** (`SURMOUNT_E2E_BASE_URL=...
   nix run .#surmount-tls-hybrid`); document negotiated group
4. **B2** `requireDeployMaterial` once PEMs (and later Arti paths) exist
5. **B3 Arti** live Tor verify + operator HS identity backup (unit
   active != published; discovery headers and live unit do **not** close B3)
6. **B4** production Nostr auth on host: **live gated (2026-08-12)**
   (anon login/401 + health 200). Apex/www packaged site live 2026-08-19
   (GitHub tip `1c84696`).
   Q-AUTH-1 still open (durable session store, key-loss, first-operator
   bootstrap UX)
7. **B5 Ban enforce lab** (sets + membership + traffic drop proof; cleanup via
   `surmount-nft-ban-helper remove-ban <ip>`)
8. **B6** mail earn-trust DNS/TLS ([docs/DNS.md](docs/DNS.md)); Track C not required
9. **B7** fail2ban shrink only after B5 solid
10. Cert renew path for chosen CA flow (Q-EDGE-1 / Q-CA-*); ACME-on-product-:80
    still parked
11. Delete `web.nix` only when dual-run unused **and** explicit operator OK
    (checklist: [docs/OPS.md](docs/OPS.md))
12. **D2 PQConnect** only after human sibling commit + Q-PQC answers
13. **Nix ssh-ng remote builder on surmount-1 (host MemoryMax live;
    Lake unit default off).**
    `surmount.remoteBuilder` + niced stdio + slice `MemoryMax` are
    switched. `nixbuilder` is key-only. `surmount.lake` exists in
    tree, enable stays false. Do not start Lake from an agent.
    Leftover: laptop system `/etc/nix/machines` plus system
    `builders=` need an operator sudo TTY (agent denied). User
    machines file is already wired. Machines slots = live guest
    max-jobs, not laptop inxi.
    SHC ticket 261 (closed 2026-08-18). Not NixOps. Mail and builder
    work share the host. Dev builds max-nice; critical units stay
    normal priority. Agents never reboot. Do not invent MX / DMARC /
    VW / ban / Q-AUTH-1 / reboot / a new SHC ticket.

### Agent-done earlier (packaging + docs; no cutover)

- [x] Version currency audit vs network latest (Stalwart family at tip;
      crane rustc 1.88 vs stable 1.97.1 noted in pre-26.05 audit; living
      crane is 1.95 via `rustPackages_1_95`, see
      [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md)). Snapshot:
      [docs/research/version-audit.md](docs/research/version-audit.md)
- [x] **Arti packaging currency:** Surmount-owned **2.5.1** source build
      (`nix/packages/arti-onion-service.nix`, GitLab `arti-v2.5.1`) +
      `onion-service-service`; rustc via `nixpkgs-rust` (MSRV 1.91+); not
      full OS channel bump. Eval contract locks version 2.5.1 (matches
      nixpkgs-rust `arti` cargoDeps; crates.io max_stable 2.5.1 as of
      2026-08-27). Live Tor verify still residual
- [x] COMPACTION-PIN dual-pin: local cleartext API for https+Arti **shipped**;
      `remove_ban` / lab unban **shipped**; residual host Tor/ban/cutover
      unchanged
- [x] Dual-run nginx unused detection checklist in OPS (no `web.nix` delete)
- [x] Local validation: `nix run .#e2e` / `just e2e`; pure helpers via
      `cargo test -p surmount-e2e --lib`; host e2e without env expected exit 2
- [x] E2E SoT rewrite: Rust `crates/surmount-e2e` + flake `apps.e2e` /
      `apps.e2e-host`; bash `scripts/e2e-*.sh` pure lib retired
- [x] Production free-:80 pin (docs dual-pin 2026-08-02; redirect-only)
- [ ] Optional local Tor deep row: still residual until built `arti` on PATH
      + Tor client + live verify (do not fake host ownership)

Do **not** invent Q-* answers (Q-ACL-1..6, Q-AUTH-1 Nostr, Q-ARTI-2/3 onion
surfaces, ACME-on-product-:80) or claim host rows from local e2e alone.

---

## Validation commands (this round)

```bash
nix run .#e2e
# or: just e2e
# local comprehensive end-to-end (hermetic); optional Tor when tools present

just check
# exit 0  -> host fmt --check + clippy -D warnings + cargo test

just check-ci
# exit 0  -> checks.<system>.ci (may be slow; includes e2e-pure-test, not host e2e)

# host only (exit 2 if SURMOUNT_E2E_HOST unset; BASE_URL required; LAB_IP unless SKIP_BAN):
SURMOUNT_E2E_HOST=1 \
  SURMOUNT_E2E_BASE_URL=https://127.0.0.1 \
  SURMOUNT_E2E_LAB_IP=203.0.113.50 \
  nix run .#e2e-host
# or: just e2e-host
# ban_drop=UNPROVEN in summary means membership/preflight only, not live drop
# lab cleanup: surmount-nft-ban-helper remove-ban "$SURMOUNT_E2E_LAB_IP"

cd crates && cargo test && cargo clippy --all-targets -- -D warnings
# pure e2e helpers + host-gate contracts without VPS:
cargo test -p surmount-e2e --lib
# module-eval only:
nix build ".#checks.$(nix eval --impure --raw --expr 'builtins.currentSystem').module-eval-contract"
```
