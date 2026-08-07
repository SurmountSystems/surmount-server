# Edge, TLS, ACME, and rate limiting

How HTTPS reaches the management UI and (optionally) Stalwart HTTP. Mail
protocol ports are **not** edge-proxied; they terminate on Stalwart. See
[STACK.md](STACK.md) for the full path map.

**Last updated:** 2026-08-02
**Operator direction:** [operator-direction.md](operator-direction.md)

- No nginx as product edge
- Prefer **first-party Axum** (or Axum + Leptos / small same-workspace edge
  crate) as the HTTPS edge
- Prefer Unix domain sockets for local hops
- **Arti onion/hidden services REQUIRED** (alongside clearnet; not optional)
- Do **not** lock ACME-only or rustls-acme-only; see
  [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md)
- Compaction reload: [COMPACTION-PIN.md](COMPACTION-PIN.md)

---

## Requirements

| Need | Notes |
|------|-------|
| TLS termination | Public HTTPS for `services`, `mail` name (certs), apex/www |
| Reverse proxy / routing | To Axum/Leptos UI; optional Stalwart HTTP path |
| Local IPC | Prefer **Unix domain sockets** to backends, not TCP localhost |
| Certificates | Automated public CA is common today; **not** locked to ACME-only (TLS research) |
| HTTP :80 | Automatically and gracefully upgrade/redirect to :443. Production: **port 80 free** for product redirect-only bind (operator 2026-08-02). ACME HTTP-01 on product :80 **parked** (Q-EDGE); dual-run nginx may still use :80 for ACME |
| TLS versions | No SSLv3, no TLS 1.0/1.1; **prefer TLS 1.3** (1.2 only if measured client need) |
| PQ where supported | Enable hybrid PQ KEX (e.g. X25519MLKEM768) when rustls/aws-lc-rs path allows; see PQC research |
| Rate limiting | Request flood protection at edge and/or app (tower-governor or equivalent) |
| Access control | Merciless ban on unauthorized access + whitelist + last-used; see access-control research |
| Direct origin | Works with DNS A/AAAA straight to the VPS |
| Implementation | Prefer **Axum-first** edge we own (operator follow-up 2026-07-30); no third-party reverse proxy as product identity |
| Ops burden | Single operator, single node; avoid k8s-shaped complexity |

## What we will not depend on

- **nginx** as the permanent product edge (security concern restated by
  operator). Current nginx is **transitional-to-delete**.
- A **separate reverse-proxy product** (Caddy, Sozu, Traefik, etc.) as the
  **default** identity. Optional later only if measured need appears.
- **Cloudflare orange-cloud** (or any CDN reverse proxy) as a **required** hop
- CF Access, Workers, WAF, or Tunnel as critical path for mail or core HTTPS
- "It only works if proxied" assumptions in modules

DNS **may** use Cloudflare or any registrar in **DNS-only** mode. Mail and
services traffic path must succeed when records point at the VPS IP.

Optional future: static marketing site behind a CDN is a separate host story
and must not become a hard dependency for this flake's mail stack.

---

## Operator preference: Axum-first edge

**Why not a separate proxy daemon by default?** Operator: we can implement
this in **Axum**; it is not that much work relative to running another product
edge. Prefer first-party code in the Surmount workspace.

**Honest scope (still real work):**

| Work item | Notes |
|-----------|-------|
| TLS terminate on :443 | rustls (or equivalent) in-process |
| Cert obtain/renew | ACME or other path; may share PEMs with mail; see TLS research |
| HTTP :80 | challenges and/or redirect |
| Rate limits | tower / governor style |
| Routing | UI vs optional Stalwart HTTP vs static legacy |
| Headers / hardening | baseline security headers, body limits |
| Local upstreams | Prefer UDS; loopback TCP until backends support UDS |

**Shape options (both Axum-family):**

1. **In-process** with management-ui (one binary listens public + serves app)
2. **Small `surmount-edge` crate** in the same workspace (Axum/tower/rustls),
   reverse-proxying to UI and other local services over UDS

Either is "first-party Axum edge." Separate proxy products are a fallback if
we measure a need we do not want to own.

Deep comparison (still useful for UDS and non-default candidates):
[research/rust-edge-and-uds.md](research/rust-edge-and-uds.md).

---

## Current state: Axum-first default + nginx dual-run escape

### Product path (default)

- `surmount.web.enable` **default false** (nginx not the product edge)
- Public HTTPS: `managementUi.listenMode = "https"` with host
  `tlsCertPath` / `tlsKeyPath` PEMs; bind public address/port as needed
  (sample comments in `hosts/mail-vps`)
- Module-eval contracts: web off => `services.nginx.enable` false; https UI
  env + MemoryDenyWriteExecute serviceConfig (no writable+executable memory);
  dual-run escape still evaluates when web on

### Transitional nginx escape (`modules/web.nix`)

- `services.nginx` + `security.acme` only when `surmount.web.enable = true`
- Vhosts for `services.surmount.systems`, `mail.surmount.systems`, apex, www
- Proxy `/` -> management UI; `/stalwart-admin/` -> Stalwart HTTP (bootstrap)
- Recommended TLS/proxy/gzip settings enabled
- `clientMaxBodySize` default 25m (mail bodies go through Stalwart, not the edge)
- Backends today are **TCP loopback** (scaffold). Target local hops are **UDS**.

**Status:** **transitional-to-delete.** Escape hatch only. Do **not** expand
nginx features. Module file remains until operators no longer need dual-run.

### Axum edge foundation (`crates/management-ui`, `modules/management-ui.nix`)

- Listen mode `http` (default bind) or `https` with host `tlsCertPath` /
  `tlsKeyPath`
- Env: `SURMOUNT_LISTEN_MODE`, `SURMOUNT_TLS_CERT`, `SURMOUNT_TLS_KEY`
- **In-process rustls HTTPS:** TLS 1.3 lean (aws-lc-rs provider; hybrid PQ KEX
  when provider defaults enable it, e.g. X25519MLKEM768). Loads PEMs from host
  paths; ALPN h2 + http/1.1. Integration test uses temp self-signed PEMs only.
- HTTP->HTTPS redirect helpers + host allowlist; optional **redirect-only**
  plain HTTP listener when `redirectHttpToHttps` + `httpRedirectListen`
  (default `0.0.0.0:80`). No cleartext API on that port. Eval mutex vs
  `web.enable` (nginx dual-run owns :80 ACME/redirect). **ACME HTTP-01 on
  product :80 is parked** (Q-EDGE; external certs / DNS-01 / dual-run ACME).
  See RESIDUAL.md §4.
- **Production assumption (operator 2026-08-02):** TCP **port 80 is free** on
  the NixOS box so the product redirect-only listener can bind. That free
  :80 is for **redirect/upgrade only**, not an invent of ACME-on-product-:80.
  Day-one ops: [OPS.md](OPS.md).
- Fixed-window rate limit; when peer is loopback, trust **X-Real-IP only**
  (useful behind dual-run nginx). X-Forwarded-For is ignored for rate-limit
  keys (leftmost XFF is spoofable). Never trust forwarding headers from
  non-loopback. Same IP key feeds the ban/whitelist layer.
- **Ban layer (first path):** `crates/management-ui/src/ban.rs` decisions
  (Allow / Whitelisted / RateLimited / Banned / BanCandidate). Whitelist never
  banned; last-used touched on allowed requests from a whitelist match.
  Enforcement via `SURMOUNT_BAN_ENFORCEMENT=off|dry-run|enforce` (default
  **off**). Optional JSON state path; optional nft add-element sync (default
  off; no CAP_NET_ADMIN on UI). Optional `nftHelper` socket-activated oneshot.
  Nix: `surmount.accessControl.*` + nft sets
  when enabled. Q-ACL-1..6 still open.
- HTTPS happy path: bare `listenMode = "https"` with PEM paths (no escape needed).
  Fail-closed if PEMs missing, unreadable, bad PEM, or key group/world readable
  (`mode & 0o077 == 0`, e.g. 0600).
- `allowCleartextHttpsEscape` / `SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1` is a
  **deliberate cleartext override** under the https label (default off; not
  production). When set, the binary **always** serves cleartext and never takes
  the rustls path, even if the acceptor is ready and PEMs are valid.
- **Default product path is nginx off.** Dual-run: set
  `surmount.web.enable = true` and keep UI on `listenMode = http` loopback.
  Tree still ships nginx module code; do not claim "nginx removed from repo."
  Host public cutover + MemoryDenyWriteExecute TLS proof remain operator
  residual (`nix run .#e2e-host`; RESIDUAL.md). Local self-signed HTTPS is `nix run .#e2e`.

### Arti HS (`modules/arti-hidden-service.nix`)

- Management-publish config: real `[onion_services."<nickname>"]` +
  `proxy_ports` to management TCP or `unix:` UDS; `storage.state_dir` =
  `onionServiceStateDir` (host deploy secrets only); no private keys or
  `.onion` addresses in git
- TCP backend: `backendAddress` null derives from
  `managementUi.listenAddress:port` when UI is plain http (tracks UI bind).
  When UI is `listenMode=https` (escape off) and no explicit
  `backendAddress` / `backendUnixSocket`, the module **auto-binds** a
  loopback cleartext full API (`SURMOUNT_LOCAL_CLEARTEXT_LISTEN`) and points
  the onion reverse-proxy at it. Auto port prefers `127.0.0.1:8090`, then
  `8091`, then `primary+1`, always avoiding `managementUi.port` and the
  active redirect port (Linux cannot bind `0.0.0.0:P` and `127.0.0.1:P`
  together). Explicit `managementUi.localCleartextListen` overrides (numeric
  loopback only: `127.0.0.1` or `[::1]`; not `localhost` hostnames).
- Lean onion backend is **cleartext** HTTP (or UDS). Pointing Arti at the
  primary https TCP (explicit `backendAddress` equal to UI listen) still
  warns. Redirect-only `:80` is never the cleartext API. No TLS-on-onion
  without a separate design.
- **Onion client IP collapse (lean TCP path):** Arti reverse-proxies to
  loopback cleartext, so the UI `ConnectInfo` peer is the local Arti process
  (`127.0.0.1`), not the onion client. Rate-limit and ban keys share one
  loopback bucket unless a future path injects a trusted client IP (Arti does
  not send `X-Real-IP` / PROXY protocol today). Do not invent headers. Park
  per-onion-client ACL / PROXY as residual; a ban of `127.0.0.1` would deny
  the whole cleartext/onion backend.
- Version assumption: Surmount-owned Arti **2.5.0** TOML shape
  (`nix/packages/arti-onion-service.nix`; GitLab `arti-v2.5.0`), not stock
  nixpkgs 1.4.2 lag. Keys: `[proxy] socks_listen`, `[onion_services]` +
  `proxy_ports`. HS package adds `onion-service-service` via owned build
- `startDaemon` default false: enable installs config + status oneshot only
- Complete lean path: `startDaemon = true` does **not** require
  `acceptIncompleteOnionConfig` (no effect this version). Package happy path:
  null `package` prefers `pkgs.artiOnionService` (passthru capable claim).
  Stock `pkgs.arti` stays fail-closed unless operator sets
  `packageIsOnionServiceCapable` (**unit active != onion published**)
- Daemon: `ConditionPathIsDirectory` + restart burst caps; HS dir must be
  writable by `surmount-arti` (e.g. 0750 surmount-arti:surmount-arti; never
  auto-create identity dir; never keys in git)
- Default lean: management backend only; Stalwart admin/JMAP onion flags
  default false and do not add stanzas yet
- Residual: live Tor verify; operator host keys/ownership; hardening after real
  `arti proxy`; local temp-key e2e != `surmount-arti` ownership
  (see RESIDUAL.md). Package overlay is in-tree; live publish is not.

Header comment in `modules/web.nix` must keep pointing here.

---

## Local IPC rules (operator direction)

```text
  Internet --:443--> Axum-first edge (TLS + certs + rate limit)
                        |
                        +--UDS--> management-ui.sock
                        +--UDS--> stalwart-http.sock   (if proxied; if engine supports)
                        +--UDS--> vaultwarden.sock     (when added)
                        +--loopback TCP--> Stalwart :8080  (scaffold / if no UDS)

  Avoid by default: edge --> 127.0.0.1:8080 style for every hop forever
  If TCP local required: nonstandard port + firewall deny from non-local
  Public standard ports: 80/443 edge only; mail ports on Stalwart directly
```

Management UI and Stalwart modules should grow UDS listen options as the edge
lands. Until then, loopback TCP remains scaffold reality. Stalwart 0.16
listener model is IP:port (see rust-edge research); do not assume UDS for
Stalwart HTTP without evidence.

---

## Candidate comparison

### Axum + tower limits + rustls (thin Surmount edge) -- preferred

| | |
|--|--|
| **Pros** | Same language and middleware story as management-ui; tower rate-limit / timeout / concurrency already idiomatic; full control; no nginx/Caddy in the TCB; matches operator "do it in Axum" preference |
| **Cons** | We own multi-vhost, renewals, edge cases, security updates of *our* edge binary; more code than a packaged proxy |
| **NixOS** | Package with crane like management-ui; simple systemd unit on :80/:443 |
| **UDS** | Natural (hyper/axum backend client to UDS) |
| **Certs** | May use rustls-acme, instant-acme, **or** load PEMs from host ACME (`security.acme`). **Not locked** to one ACME crate. |
| **Verdict** | **Default path** per operator follow-up 2026-07-30 |

### Rama / Pingora / other Rust proxy frameworks

| | |
|--|--|
| **Verdict** | **Optional later** if proxy depth outgrows a thin Axum edge and measurement says so. Not the first cutover default. |

### Caddy / Sozu / Traefik / HAProxy / Envoy

| | |
|--|--|
| **Verdict** | Not default. Emergency bridge only if Axum edge slips and nginx must die sooner. Wrong complexity class for one mail VPS in several cases (Traefik/Envoy). |

### Stay on nginx forever

| | |
|--|--|
| **Verdict** | **No.** Bridge only. Operator restated security concern. Mark delete and migrate. |

---

## Recommendation

| Horizon | Choice |
|---------|--------|
| **Now (tree default)** | **Axum-first** management-ui rustls (`listenMode=https` + host PEMs); `surmount.web.enable` default **false** |
| **Dual-run escape** | `surmount.web.enable = true` + UI `listenMode=http` loopback; nginx + `security.acme` (**transitional-to-delete**) |
| **Production :80** | **Free** for product **redirect-only** bind (operator 2026-08-02). Not ACME-on-product-:80 (parked; Q-EDGE) |
| **Host residual** | Public :443 + MemoryDenyWriteExecute (no writable+executable memory) + cert path (Q-EDGE-1 / Q-CA-*); see RESIDUAL.md host tracks. Not claimed done from eval or local e2e alone |
| **Local backends** | Move UI (and proxied Stalwart HTTP when possible) toward **Unix domain sockets** |
| **Not target** | Caddy/Sozu/nginx as product identity |
| **Always** | Direct-to-VPS DNS; no CF required path; mail ports stay on Stalwart |

### Cutover sketch (remaining / historical)

Tree default is already Axum-first (nginx off). Remaining operator work is host
proof, not flipping the module default again.

1. Place host certificate and key files (PEMs); set `listenMode=https` + public
   bind; leave `web.enable` false. Loud gate: `secrets.requireDeployMaterial` +
   PEM `requiredHostPaths`.
2. Prefer Unix socket upstreams over time; loopback TCP remains scaffold.
3. Cert material for `mail.surmount.systems` must stay usable by Stalwart for
   IMAPS/SMTPS (shared cert dir or documented export). Avoid trapping mail TLS
   only inside edge-private storage without a recovery story.
4. Host end-to-end: `SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=... just e2e-host`
   (and `SURMOUNT_E2E_LAB_IP=...` unless `SKIP_BAN=1`). Proves unit active,
   MemoryDenyWriteExecute, curl `/health` over TLS, nginx inactive when web
   off. Full recipes below.
5. Dual-run only while migrating: `web.enable = true` + UI http loopback.
   Module asserts against dual-run + public UI https (:443 or non-loopback).
6. When dual-run is unused: remove nginx module path; update STACK, OPS,
   hygiene, architecture-review.
7. Lock down or remove `/stalwart-admin/` public path once Rust admin covers
   bootstrap needs.

### Host end-to-end recipes (operator)

**Local first:** `nix run .#e2e` / `just e2e` proves the product path with
**self-signed** temp certificate/key files, shared-router health/SSR/rate-limit/ban
contracts, and (when arti + Tor client exist) optional local hidden-service
publish with temp keys. That is not public cutover.

**Host mode** refuses to run without env (exit 2) so unset env never looks like
a host pass:

```bash
# On the VPS (or against a staging deploy from a laptop):
export SURMOUNT_E2E_HOST=1
export SURMOUNT_E2E_BASE_URL=https://127.0.0.1   # required (health cannot be silently skipped)
export SURMOUNT_E2E_LAB_IP=203.0.113.50           # required unless SKIP_BAN; lab only
# optional:
# export SURMOUNT_E2E_TLS_HOST=services.example:443
# export SURMOUNT_E2E_TLS_SNI=services.example
# export SURMOUNT_E2E_ONION=....onion    # published proof (unit active alone is not enough)
# export SURMOUNT_E2E_SKIP_BAN=1         # HTTPS+Arti only; summary ban_drop=skipped
nix run .#e2e-host
# or: just e2e-host
```

| Track | What host mode checks | Notes |
|-------|----------------------|--------|
| **1. HTTPS + hardening** | **`BASE_URL` required** (FAIL if unset); health 200; UI unit active; `MemoryDenyWriteExecute=yes` (no writable+executable memory); TLS check (handshake; **expired cert fails**); nginx inactive; UI ambient **and** bounding set have no CAP_NET_ADMIN | Operator PEMs on host; key not group/world readable; external PEMs lean until Q-EDGE/Q-CA answered; ACME-on-product-:80 parked |
| **2. Arti** | `surmount-arti-hidden-service` (or override) active; `User=surmount-arti`; optional onion fetch via `SURMOUNT_E2E_ONION` + `TOR_SOCKS` | **Unit active != published.** HS keys under `onionServiceStateDir`, never in git. Cleartext local backend when UI is https-only. Do not invent Q-ARTI-2/3 |
| **3. Ban / kernel firewall** | Helper socket + helper socket unit; set **preflight** (exists only, not drop); **`LAB_IP` required** and must appear in `surmount-ban4` (FAIL if unset/absent) unless `SKIP_BAN=1` | CAP_NET_ADMIN on **helper only**. Summary `ban_drop=UNPROVEN` (membership != traffic drop). Prove drop + cleanup out-of-band. Q-ACL-1..6 parked |

Harness SoT: `nix run .#e2e-host` (Rust `crates/surmount-e2e`). Standalone TLS
ops: `scripts/check-tls.sh` (exit non-zero on expired cert). Host e2e is never
a default flake check.

---

## TLS trust and ACME (not locked)

Public CA + ACME is the **common** path today and remains a strong
compatibility default for browser HTTPS. It is **not** the only option and
is **not** locked for product law this turn.

Read: [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md)

Covers: public CA risk, private CA, DANE/TLSA for mail, short-lived certs,
pinning limits, performance (handshake, OCSP, renewal), and the split between
**mail TLS** and **browser HTTPS**. Open ids **Q-TLS-1** ...

### Public CA candidates (automated; not locked)

Open to a better CA than Let's Encrypt if automated and not too pricey:

| CA | Notes |
|----|-------|
| Let's Encrypt | Common ACME default; free; rate limits |
| ZeroSSL | ACME; free tier + paid |
| Buypass | ACME-capable public CA |
| Google Trust Services | ACME; public WebPKI |
| Commercial ACME API | DigiCert, Sectigo, etc. when paid path is worth it |

Ids: **Q-CA-1**, **Q-CA-2** in [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).

### TLS protocol and PQ cipher posture (operator direction)

| Rule | Lean |
|------|------|
| :80 -> :443 | Graceful redirect/upgrade on free production :80 (redirect-only product path). ACME HTTP-01 may share :80 only on dual-run nginx or a future Q-EDGE answer; **not** invent ACME-on-product-:80 |
| Insecure protocols | Disabled: SSLv3, TLS 1.0, TLS 1.1 |
| Prefer | TLS 1.3 |
| PQ KEX on Axum/rustls | **First-class:** prefer **aws-lc-rs** provider; enable hybrid groups such as **X25519MLKEM768** when wiring the edge (rustls 0.23.x documents these groups) |
| OpenSSL 3 | Relevant for host tools / some mail stacks; not the default in-process edge |
| PQConnect | **Separate** E2EE PQC path layer; first-class research priority; does not replace TLS hybrid KEX |
| Cloudflare | **Research only.** We may cite CF Research blog posts on hybrid PQ TLS / migration; we do **not** run CF products as edge or CDN |

Detail: [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md)
(includes plain URLs to CF Research PQ posts and the TLS-hybrid vs PQConnect
split).

### Arti onion / hidden services (REQUIRED; not the clearnet edge)

**Arti** (Tor Project Rust Tor) **onion/hidden service** reachability is
**required** product surface for Surmount Server services (operator direction
2026-07-30). First-class **alongside** clearnet where clearnet applies. Still
no Cloudflare products; own stack.

Arti is **not** a substitute for the Axum clearnet HTTPS edge. HS identity keys
and startup material stay in **deploy secrets** on the host (**never in git**);
human inventory via Vaultwarden (password manager API; Vaultwarden does **not**
implement Bitwarden Secrets Manager API today). Relay/egress remain separate
open choices. Module in tree: `modules/arti-hidden-service.nix` (management-
publish config + optional daemon). Service-capable package:
`pkgs.artiOnionService`. Residual: live Tor verify on host, operator host keys.
Local optional Tor row: `just e2e` (temp keys; not host ownership).

Research: [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md).
Pin: [COMPACTION-PIN.md](COMPACTION-PIN.md) section 7.

---

## Rate limiting and merciless access control

Layers (defense in depth):

1. **nftables (opt-in):** when `surmount.accessControl.enable` + `nftSets`,
   table `inet surmount_guard` with sets `surmount-ban4` / `surmount-ban6` /
   `surmount-whitelist4` / `surmount-whitelist6`. Whitelist accept early; ban
   drop. Sets start empty; operator must load/verify on host.
2. **Edge rate limit:** fixed-window per client IP key (in-memory).
3. **Edge ban decide:** same IP key; enforce mode returns 403 for banned;
   default enforcement **off**.
4. **App (Axum):** Unauthorized -> ban is a **thin BanCandidate hook**
   (`BanGuard::signal_unauthorized` + request-context `signal_unauthorized`);
   records under DryRun/Enforce; whitelist never banned. **Wired** for
   session-exchange parse/verify fail and bad presented NIP-98 on protected
   paths (not missing cookie; not 404/501). Matrix: [SECURITY.md](SECURITY.md)
   *Auth failure to ban matrix*. **Q-ACL-1** (full surface list beyond auth)
   still open.
5. **Stalwart:** built-in anti-abuse, greylisting, spam-filter (mail plane).
6. **Ban policy (operator direction):** unauthorized access of intentional
   auth/probe class -> **immediate or near-immediate blacklist** on the box.
   **Whitelist** IPs never banned; track **last-used** for hygiene.
7. **fail2ban today:** light sshd sketch only (`hardening.nix`); transitional.
   Long-term prefer **Rust** + nft over Python fail2ban as product identity.

Design + open Q-ACL-*:
[research/access-control-fail2ban.md](research/access-control-fail2ban.md).

### Operator end-to-end (ban / rate-limit first path)

Hermetic local (no secrets, no root kernel firewall required):

```bash
just e2e
# or focused cargo:
just test
cd crates && cargo test ban::
```

Module contracts (firewall set names + lean defaults):

```bash
just check   # includes module-eval accessControl tests
```

Host (operator; not claimed done by tree or local e2e alone):

1. Set `surmount.accessControl.enable = true` (and optional whitelist CIDRs).
2. Confirm kernel firewall loaded: `nft list table inet surmount_guard` shows
   empty ban/whitelist sets with the canonical names above
   (`surmount-ban4` / `surmount-ban6` / whitelist sets).
3. Optional app enforce without host drop: `enforcement = "enforce"` +
   backend memory; curl from a test IP after a manual ban signal should 403.
4. Prefer host drop with **all** of: `backend = "nft"`,
   `enforcement = "enforce"`, **`nftHelper = true`**, and absolute `nftBin`.
   UI connects to `SURMOUNT_BAN_NFT_HELPER_SOCK=/run/surmount/nft-ban-helper.sock`
   and a socket-activated oneshot (`surmount-nft-ban-helper@`) runs with
   CAP_NET_ADMIN/RAW. **`nftHelper` requires `backend = "nft"`** (Nix
   assertion); Memory + helper is fail-closed, not a silent sock install.
   The UI unit keeps NoNewPrivileges and never gets CAP_NET_ADMIN; do **not**
   rely on child setcap spawn (blocked by NNP). Do **not** enable `nftExec`
   on the UI unit (mutually exclusive; module warning). DryRun never calls
   the helper. Live host proof that set elements drop traffic:
   `SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=... SURMOUNT_E2E_LAB_IP=... just e2e-host`
   (BASE_URL required; lab IP only; cleanup required; or `SKIP_BAN=1` for
   HTTPS-only). App-level `enforcement = "enforce"` + `backend = "memory"`
   works without host firewall (step 3).
5. Rate limit: burst past `rateLimitMaxRequests` => 429 + Retry-After. Enforce
   bans short-circuit before rate counters. Redirect-only :80 shares the same
   ban/rate middleware (403/429 only; no cleartext API).
6. Behind dual-run nginx: only **X-Real-IP** from loopback is trusted for keys;
   XFF is ignored.

Do not pretend edge rate limits replace mail authentication and spam policy.
Spam detection is **first-class** product priority (operator direction).
Do not ban on every mail greylist event.

## Ports reminder

| Port | Role | Edge? |
|------|------|-------|
| 80 | ACME HTTP-01 and/or redirect (if using that path) | Yes |
| 443 | HTTPS | Yes |
| 25/465/587/993/4190 | Mail | No (Stalwart) |
| UI / Stalwart HTTP | Local only | UDS preferred; loopback TCP scaffold |

## Open questions

**Q-EDGE-1.** Shared host ACME PEM files for mail + web vs in-process issuance
on the Axum edge for web only?

**Q-EDGE-2.** UDS path layout under `/run/surmount/` vs `/run/` service-specific
dirs?

**Q-TLS-1** ... cert trust choices: [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md).

**Q-CA-1** / **Q-CA-2.** Primary public CA and multi-CA failover:
[research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).

**Q-PQC-1** ... PQConnect timing and mail PQ path: same research note.

**Q-ACL-1** ... merciless ban definition and whitelist store:
[research/access-control-fail2ban.md](research/access-control-fail2ban.md).

(Axum-first vs separate proxy product: **answered** -- Axum-first preferred.)

## Related

- `modules/web.nix` - transitional nginx (delete after cutover)
- `modules/networking.nix` - firewall
- `modules/hardening.nix` - fail2ban sketch + optional accessControl nft sets
- `crates/management-ui/src/ban.rs` - ban decide + backends
- [operator-direction.md](operator-direction.md)
- [principles.md](principles.md)
- [SECURITY.md](SECURITY.md)
- [OPS.md](OPS.md) - checks and runbooks
- [research/rust-edge-and-uds.md](research/rust-edge-and-uds.md)
- [research/tls-trust-and-acme.md](research/tls-trust-and-acme.md)
- [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md)
- [research/arti-and-secrets-manager.md](research/arti-and-secrets-manager.md)
- [research/access-control-fail2ban.md](research/access-control-fail2ban.md)
- [research/luks2-and-deploy-secrets.md](research/luks2-and-deploy-secrets.md)
- [DNS.md](DNS.md) (mail earn-trust / DANE checklist)
