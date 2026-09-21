# Edge, TLS, ACME, and rate limiting

How HTTPS reaches the management UI and (optionally) Stalwart HTTP. Mail
protocol ports are **not** edge-proxied; they terminate on Stalwart. See
[STACK.md](STACK.md) for the full path map.

**Last updated:** 2026-09-20 (`splora.surmount.systems` is a portal Host:
GET / lists the live non-mainnet indexer Hosts as https links; that Host
is on the :80 allowlist and gets a per-site onion. Not a fifth indexer.
Not mainnet.) Prior 2026-09-11 (each public HTTP Host gets its own v3 onion;
Onion-Location is `http://{that-host-onion}{path}`; mail Hosts stay
unmapped). Prior 2026-09-07 (intended production leaf is **one** Let's
Encrypt PEM pair (`with_single_cert`) covering **20** certificate
hostnames: the live 18 plus `cryptoquick.com` and `www.cryptoquick.com`.
Live leaf as of this measure still has **18** names (CT): extra static
six zones apex+www, surmount apex/www/mail/services/mta-sts, and
`mail.cryptoquick.com`. Missing: `cryptoquick.com` and
`www.cryptoquick.com`. That mismatch is why cryptoquick HTTPS fails
verify. Validating A for cryptoquick apex/www succeeds (AD true).
Leftover parent DS key tag 2368 is gone. The old wait-for-SERVFAIL gate
is closed as a live A-lookup gate. SHA-1 parent DS digest type 1 remains
standing DNSSEC quality debt in operator-facts Monday leftover; it is
not the HTTPS cause. Esplora Hosts stay off this leaf. Do not invent
leftover Namecheap clicks. Do not MX-flip Baxter.) Prior 2026-09-03 (HTTP/3 NIP-07 login 500: axum-h3 omits Axum `ConnectInfo` on `POST /api/v1/auth/session`; optional peer, do not ban unspecified). Prior 2026-09-02 (Splora REST requires a Bitcoin JSON-RPC peer by design; indexer is not Core; remote shape is `daemonDir = null` plus cookie path plus `daemonRpcAddr`; node inventory is tasked in the splora tree). Prior same day (flake input `splora` locked to `9481e4cb87273aa99b0357be48503765beadb919`; `surmount.sploraIndexer` stays the host-local single knob over first-class instance JSON-RPC options; five esplora Hosts and UDP 443 / HTTP/3 stay optional Axum edge, not mempool REST prerequisites; Unix socket or one existing Host is enough; do not map REST onto the mail console Host). Prior 2026-09-01 (`surmount.sploraIndexer` host-local wrap for one remote JSON-RPC indexer). Prior 2026-09-01 (flake input `splora` locked to `343727487988ed0a764674ff21c0750465b9a3e8`; overlay consumes input packages; no this-tree crane wrap). Prior 2026-09-01 (Splora Host map: TCP HTTP/2 vs UDP HTTP/3; leftover esplora certificate hostnames as complete sentences). Prior 2026-09-01 (flake input `splora` on the `surmount` branch). Prior 2026-09-01 (documented esplora Hosts for the Splora Unix proxy; flake input `splora`). Prior 2026-08-31 (splora Unix sockets behind Axum; HTTP/3 QUIC on UDP :443). Prior 2026-08-25 (operator bins are `nix run .#...`.) Prior 2026-08-24 (`just deploy` publishes static sites; `just deploy-host` is the NixOS generation.) Prior 2026-08-21 (live production leaf was 18 certificate
hostnames: six extra static zones apex+www plus surmount apex, www,
mail, services, mta-sts, plus **mail.cryptoquick.com** for IMAP/SMTP.
Extra-vhost HTTPS live for those six. `cryptoquick.com` apex/www stayed
off the leaf that day. Prior 2026-08-18: laptop Let's Encrypt renew is
`just laptop-renew-cert -- --check|--live --directory production`.
`--live` issues only when due. Host ACME stays off. Stalwart 0.16.15
query resolves certificate hostnames plus id.)
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
| Routing | UI vs optional Stalwart HTTP vs static legacy vs optional Vaultwarden `/vault/` vs splora Hosts |
| Headers / hardening | baseline security headers, Onion-Location + onion `h2` Alt-Svc on mapped HTTPS, clearnet `h3` Alt-Svc only when QUIC is bound, body limits |
| Local upstreams | Prefer UDS; loopback TCP until backends support UDS |
| Vaultwarden subpath | Optional Axum reverse-proxy of loopback Rocket under `/vault/` (no nginx, no new subdomain) |
| Splora | Optional Host -> `/run/splora/<instance>.http.sock` (HTTP/1.1); queue POST on its own socket; no Electrum newline proxy |
| HTTP/3 | UDP :443 QUIC next to TCP :443; same PEMs; QUIC ALPN h3 only. axum-h3 does not insert Axum `ConnectInfo`; NIP-98 session POST must not 500 for that (optional peer; do not ban unspecified). |

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

### P1: Axum owns public :80 / :443 (operator lock)

**Operator approval:** Rust HTTP server (Axum management-ui) is the clearnet
HTTPS frontend on **:80** and **:443**. Stalwart is the **backend mail**
engine (SMTP/submission/IMAP/ManageSieve and related). Stalwart is **not**
the permanent product clearnet HTTPS owner on :443.

| Plane | Owner | Notes |
|-------|--------|------|
| Public :80 | management-ui redirect-only (or dual-run nginx ACME escape) | Free production :80 is operator-approved for product redirect |
| Public :443 TCP | management-ui rustls (`listenMode=https` + host PEMs or in-process ACME) | Product browser / services HTTPS (`h2` + `http/1.1`) |
| Public :443 UDP | management-ui QUIC HTTP/3 (same PEMs, ALPN `h3`) | First-class with TCP; firewall UDP 443 when UI HTTPS is on |
| Mail 25/465/587/993/4190 | Stalwart | Same durable Let's Encrypt PEMs as Axum (`/var/lib/surmount/secrets/tls/{cert,key}.pem`) after day-2 Certificate apply. First-boot still inserts an engine self-signed leaf until that apply. |
| Stalwart HTTP management | Loopback (prefer `127.0.0.1:8080`) | SSH tunnel / bootstrap; not public product edge |
| Stalwart first-boot HTTPS :443 | Temporary engine default until removed | Free with apply plan before public B1 |

**Offline free-:443 path (shipped):**

- Template: `nix/stalwart/free-public-443-for-axum-edge.ndjson`
- Host install: `/etc/surmount/stalwart/` (via `modules/mail.nix` when
  `surmount.enable`)
- Runbook: `nix/stalwart/README.md` and
  `/etc/surmount/stalwart/README-free-public-443.txt`
- Operator: `stalwart-cli query NetworkListener` then
  `apply --dry-run` / `apply` (token host-only). Confirm first-boot listener
  **names** match the template (`name=https`); adjust host-local copy if not.
- Sample host comments: `hosts/mail-vps/configuration.nix` (do not enable
  public UI https until :443 is free for Axum)

**Honesty:** first boot with empty RocksDB may still bind engine HTTPS :443
until apply/WebUI. Tree defaults do **not** open Stalwart :443 in
`services.stalwart.openFirewall`. Live public cutover (PEMs/ACME, DNS,
`just e2e-host`) remains host residual (RESIDUAL.md). Not claimed done from
docs or module-eval alone.

### Product path (default)

- `surmount.web.enable` **default false** (nginx not the product edge)
- Public HTTPS: `managementUi.listenMode = "https"` with host
  `tlsCertPath` / `tlsKeyPath` PEMs; bind public address/port as needed
  (sample comments in `hosts/mail-vps`)
- Stalwart does **not** permanently own product clearnet :443 (P1 above)
- Module-eval contracts: web off => `services.nginx.enable` false; https UI
  env + MemoryDenyWriteExecute serviceConfig (no writable+executable memory);
  dual-run escape still evaluates when web on; free-:443 plan installed under
  `environment.etc`

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

- **Auth on the public edge (2026-08-12):** public product HTTPS on the
  **services** Host (operator console) **requires** `authMode = "nostr"` with
  host session secret + allowlist. `authMode = "off"` is **loopback / lab
  only**. When mode is off, middleware does not gate routes (full anonymous
  console). Product footgun guard (binary + Nix) may refuse public primary
  edge with auth-off; lab keeps loopback auth-off. Host enable runbook and
  proof curls: [OPS.md](OPS.md) *Production Nostr auth on the public edge
  (B4)*. Posture: [SECURITY.md](SECURITY.md). Secrets: [SECRETS.md](SECRETS.md).
  **Live B4 (2026-08-12):** public services is Nostr-gated (anon `/` login,
  `/api/v1/domains` 401, `/health` 200). Residual **B4** is live-gated;
  Q-AUTH-1 still open. Report: `.agents/reports/impl-auth-live-b4-switch.md`.
- **Interim day-1 (until PEMs + public :443):** bind cleartext UI on
  **loopback only** (`surmount.managementUi.listenAddress` default
  `127.0.0.1`, port default **8090**). Default `authMode = "off"` is not
  public-safe. Do not set a public `listenAddress` with auth off. Reach via
  SSH tunnel; pin loopback in private host-local if needed. Product path is
  still Axum owning public :80/:443 with real TLS (B1), then **B4** Nostr
  before treating services as production-safe.
- **Recommended with public https:** private host-local also sets
  `publicBaseUrl = "https://services.<domain>"` so NIP-98 `u` tags match the
  public origin (optional empty = request Host + scheme).
- Listen mode `http` (default bind) or `https` with host `tlsCertPath` /
  `tlsKeyPath`. **H-PEM durable default** for new host profiles:
  `/var/lib/surmount/secrets/tls/{cert,key}.pem` (cert mode **0640**
  `surmount-ui:surmount-tls`; key mode **0600** owner-only `surmount-ui`)
  and account JSON under `.../acme/account.json` (mode 0600). Stalwart
  mail-plane TLS uses copies under `.../mail/tls/` (key 0600
  `stalwart-mail`). Ephemeral `/run/surmount-secrets/...` remains a valid
  override (wiped on reboot).
- **Multi-name host profile (H5):** private profile `acme_domains` lists
  certificate hostnames for the issued cert (sample: services + **mail** +
  apex + www). **`mail` is first-class** so IMAP/SMTP can present a
  browser-trusted name. Render writes a **space-separated** Nix list. Live
  issuance is **laptop DNS-01** (Namecheap; ClientIp = laptop egress). Host
  ACME stays off. Optional `mta-sts.<domain>` on the private profile when
  the policy host is live. Named renew: `just laptop-renew-cert -- --check`
  or `--live --directory production` (directory required; never silent
  staging/prod). `--live` issues only when the leaf is due; otherwise it
  restages the matching pair. Laptop user timer:
  `--install-timer` (not a VPS ACME timer).
- **Mail-plane TLS (Stalwart-owned copies):** Stalwart does **not** take
  IMAP/SMTP certs from Nix `settings` (that option is ignored). Point the
  engine at copies under `/var/lib/surmount/secrets/mail/tls/` with
  `just point-stalwart-mail-tls` (default dry-run; `--live --restart` on
  the host). Axum keeps `/var/lib/surmount/secrets/tls/` with key mode
  **0600**. Stalwart 0.16.15 query JSON is certificate hostnames plus `id`
  (not `filePath`); the driver resolves the Let's Encrypt File leaf that
  way. Nix grants `ReadOnlyPaths` for `secrets/mail/tls` (and still
  grants `secrets/tls`). Template:
  `nix/stalwart/mail-plane-tls-le-pems.example.ndjson`. Host ACME stays
  **off**. Proof: `nix run .#e2e-host` (mail TLS rows when BASE_URL is set;
  issuer must not be `rcgen self signed cert`; hostname list must include
  `mail.<apex>`). Evolution "Accept Permanently" is not the product fix.
- **Production LE directory (H6):** scaffold default is Let's Encrypt
  **staging**. Production directory is never silent: re-render with
  `nix run .#surmount-render-host-profile-acme -- --directory production` (preferred;
  does not rewrite the profile file) **or** set profile `acme_directory` to
  the production URL explicitly. Cutover step `le-prod` forces that explicit
  path. Re-render of a staging profile without a flag stays staging.
- Env: `SURMOUNT_LISTEN_MODE`, `SURMOUNT_TLS_CERT`, `SURMOUNT_TLS_KEY`
- **In-process rustls HTTPS:** TLS 1.3 lean (aws-lc-rs provider; workspace
  feature `prefer-post-quantum` so default provider offers hybrid
  **X25519MLKEM768** first). Loads PEMs from host paths; ALPN h2 + http/1.1.
  Hermetic unit tests prove provider includes hybrid and prefers it first;
  integration test uses temp self-signed PEMs only.
  **Honesty:** hybrid group config is a **tree/provider proof** (D1 offline).
  Do **not** claim a live production host negotiated X25519MLKEM768 without
  host proof after B1 HTTPS cutover. Operator probe (env placeholders only):
  `SURMOUNT_E2E_BASE_URL=https://... nix run .#surmount-tls-hybrid` (exit **2**
  **BLOCKED** when unset; never silent skip success; not a flake check).
  Runbook: [deploy-host-local.md](deploy-host-local.md) section 6 (D1-host).
  PQConnect (D2) is a separate path layer; see
  [research/pqconnect-and-pqc.md](research/pqconnect-and-pqc.md).
- HTTP->HTTPS redirect helpers + host allowlist; optional **redirect-only**
  plain HTTP listener when `redirectHttpToHttps` + `httpRedirectListen`
  (default `0.0.0.0:80`). No cleartext API on that port. Eval mutex vs
  `web.enable` (nginx dual-run owns :80 ACME/redirect). **ACME HTTP-01 on
  product :80 is parked** (Q-EDGE; free :80 is redirect-only, not an invent of
  ACME-on-product-:80). See RESIDUAL.md.
- **Apex / www public main site vs services console (2026-08-12; document
  root 2026-08-14; SurmountSystems/site 2026-08-18):** allowlisted Host
  `primaryDomain` or `www.<primaryDomain>` is the **public** main site.
  When `SURMOUNT_APEX_PUBLIC_ROOT` (Nix `managementUi.apexPublicRoot`)
  points at a directory that contains `index.html`, those static files are
  served (no directory listing; path traversal 404). The management-ui
  module mkDefaults this to `pkgs.surmount-public-site` (flake input
  `github:SurmountSystems/site`; operator bumps the locked rev). A host
  directory such as `/var/lib/surmount/public-site` remains a valid
  override. Missing root or missing `index.html` keeps the yellow
  **UNDER CONSTRUCTION** page (`#FFFF00`). **Live 2026-08-19:**
  apex and www serve the current GitHub site copy, not UNDER CONSTRUCTION.
  `just deploy` publishes those static files (and extra vhosts); `just deploy-host`
  is the NixOS generation. Services
  console is unchanged.
  No operator nav on apex/www. Operator console (Dashboard Overview, Stalwart
  chips) is only on **`servicesHostname`** (e.g. `services.surmount.systems`).
  HTTP :80 same-host HTTPS upgrade for every allowlisted Host, including
  apex/www (public users do **not** get dumped onto the services console).
  Apex still answers `/health` and `/.well-known/*`; apex `/api/*` (except
  `/api/health`) is 404. Public-site CSP allows `'unsafe-inline'` scripts so
  existing support.html clipboard JS works; the services console keeps the
  nonce CSP.
  Open-redirect safe: empty allowlist or non-listed Host never builds a
  Location from untrusted input. Default allowlist: services, apex, www,
  mail, mta-sts.<primary>, plus any extra static Hosts from
  `SURMOUNT_STATIC_VHOSTS` / `SURMOUNT_STATIC_VHOSTS_FILE`.
- **Extra static Hosts (DS3018xs trees, 2026-08-19):** proven public HTML
  sites are served from the same Axum process via a Host -> document root
  map (`SURMOUNT_STATIC_VHOSTS` JSON, or Nix
  `surmount.managementUi.staticVhosts`). Same path/MIME/CSP/traversal
  rules as apex. Extra Hosts serve document-root `/.well-known/*`
  (closed 404 if missing); `/health` and `/api/health` stay edge probes;
  never the operator console. Do **not** overload `SURMOUNT_APEX_PUBLIC_ROOT`.
  `services.surmount.systems` stays the operator console. Extra Hosts
  each get a distinct v3 onion (2026-09-11), not a shared onion.
  Onion-Location for extra Hosts, apex/www, and `mta-sts.{primary}` is
  `http://{that-host-onion}{path}` (empty path becomes `/`). The services
  onion root stays the console. Mail stays unmapped. Do not put nginx
  back as product edge.

### Splora Host -> Unix socket map

The public sample host keeps `surmount.sploraProxy.enable` at the default
**off**. Private host-local enables the proxy. Indexer sockets exist only
when those indexer units run.

Mempool / Esplora REST is the indexer process on a Unix socket (or TCP
`--http-addr`). It does **not** require public Hosts, Let's Encrypt names,
or HTTP/3. A local client can call
`curl --unix-socket /run/splora/<instance>.http.sock http://localhost/blocks/tip/height`
(plus NIP-98 unless `--public-health` and that exact tip path). Unix
socket or **one existing** Host on the already-running Axum listener is
enough. Do **not** map REST onto the mail console Host
(`services.surmount.systems`); that Host would steal every path. Do not
invent a `/esplora` prefix on the console.

The five `esplora.*` names below are an **optional** documented Host map,
not a REST gate. They are not on the live leaf today. Adding them to the
production leaf is optional edge work. Do not invent extra domains this
turn. Do not treat a Host map in this file as proof the certificate
already covers them.

Clearnet clients that use this optional map reach the edge on TCP :443
(TLS 1.3, ALPN `h2` and `http/1.1`) and, when HTTP/3 is on (the
`http3Enable` default), on UDP :443 (QUIC, ALPN `h3` only). HTTP/3 and
hypervisor UDP 443 are optional edge, not prerequisites of the mempool
REST. The hop from this process to each Splora Unix socket is still
HTTP/1.1. A client that arrived on HTTP/3 does not make the indexer
speak HTTP/3. Splora does not listen on UDP.

| Network | Public Host | Unix socket | Notes |
|---------|-------------|-------------|-------|
| portal | `splora.surmount.systems` | (none) | GET / HTML lists live non-mainnet Hosts as https links. Not an indexer. |
| mainnet | `esplora.surmount.systems` | `/run/splora/mainnet.http.sock` | Indexer HTTP + `GET /api/v1/ws`. Stays off unless host-local turns it on. |
| testnet3 | `testnet3.esplora.surmount.systems` | `/run/splora/testnet3.http.sock` | Same |
| testnet4 | `testnet4.esplora.surmount.systems` | `/run/splora/testnet4.http.sock` | Same |
| mutinynet | `mutinynet.esplora.surmount.systems` | `/run/splora/mutinynet.http.sock` | Same |
| liquid | `liquid.esplora.surmount.systems` | `/run/splora/liquid.http.sock` | Same |
| queue | `POST /splora/queue` (any Host) | `/run/splora/queue.sock` | `{npub,email}` only; not indexer |
| Electrum newline socket | **not proxied** | (no socket) | Fail-closed if configured |

  **Live in browsers (2026-08-20):** Namecheap NS, exclusive A matching
  this host, HTTPS **200**, production leaf covers apex+www, packaged
  site content, not console, not COMING SOON:
  yiffa.app, baxterartworks.com, btcfur.com, iantuckerstudios.com,
  nostrfurs.com, exophiles.org (each apex + www). Host-local roots under
  `/var/lib/surmount/static-sites/<slug>`. `baxterartworks.com` is also a
  local Stalwart Domain; public MX still parked. Static-site-only extras
  are **not** mail domains.

  **Not live in browsers as trusted HTTPS (2026-09-07):**
  `cryptoquick.com` and `www.cryptoquick.com`. Validating A lookups
  succeed (AD true). Leftover parent DS key tag 2368 is gone. The live
  production leaf still has **18** names and does **not** include those
  two. HTTPS fails at certificate hostname mismatch on the same Axum
  listener and the same Let's Encrypt YE2 leaf. Intended leaf is **20**
  names on one PEM (`with_single_cert`): the live 18 plus those two.
  Host tree is populated; insecure `-k` can still return packaged site
  titled Hunter Beast. Do **not** invent leftover Namecheap DNSSEC
  clicks. SHA-1 parent DS digest type 1 remains standing DNSSEC quality
  debt in operator-facts Monday leftover; it is not the HTTPS cause.
  Esplora Hosts stay off this leaf. Do **not** MX-flip Baxter.

  Not extra Hosts (do not Host-serve): btcdragonlord.com (not
  operator-owned), btckitties.com (archived), denver.space,
  justsaybits.org.

  Skipped (not static or uncertain): divdurv.art Ghost, lunarlupine Grav,
  hoverbyte.com, bips.dev/bip360, denverbitdevs.com, dues.denver.space,
  donate.denver.space, www.ro.me, biggaymonster, cryptoquick
  mail/git/ipfs* service labels.

  Populate roots from DS3018xs GVFS (laptop AFP, never print office LAN
  IPv4): `just diskstation-afp-mount -- --host DS3018xs --share sites`
  then `just sync-static-sites-from-ds3018xs -- --live --target USER@HOST`.
  Those six extra zones already have DNS + production Let's Encrypt
  certificate hostnames. `https://<hostname>/` and
  `https://www.<hostname>/` are live **200**. Do not re-issue just to
  add a name that is already on that leaf. **Live 2026-08-21 through
  2026-09-07:** `mail.cryptoquick.com` is on this shared production leaf
  (IMAP :993 / SMTPS :465 identity). Cryptoquick apex/www are first-class
  web Hosts like the six extra static zones. **Live 2026-09-07/08:**
  production leaf is **20** names on the same PEM (`with_single_cert`),
  including `exophiles.org` and `www.exophiles.org`. Do **not** omit
  Namecheap static zones when adding cryptoquick apex/www. Pass an
  explicit `--domains` list of those 20 names so host-profile Cloudflare
  extras (`btcdragonlord.com`, `btckitties.com`) cannot sneak in.
  `--live` does **not** detect missing certificate hostnames.
  Extra-zone DNS-01 uses the dispatcher hook; the laptop `--issue` wrap
  must re-export `HOME` (ACME `env_clear`) so dispatch can find
  `namecheap/<sld.tld>.env`.

  Adding a name: copy existing apex/www A/AAAA with
  `just dns-zone-namecheap -- --credentials FILE -- list` then
  `set-host @` and `set-host www`. Laptop ClientIp (laptop egress
  whitelist). Then add FQDNs to private `acme_domains` and
  `just laptop-renew-cert -- --issue --directory production --host-profile ~/.local/share/surmount/host-profile.toml --install --restart-ui --target USER@HOST`.
  `--live` does **not** detect missing certificate hostnames (expiry
  only). Do not start host in-process ACME.

  Extra-zone DNS-01 (cryptoquick.com, yiffa.app, and other Namecheap
  zones besides surmount.systems) cannot use the one-SLD Namecheap hook
  alone. Pass the dispatcher as `--hook`:
  `nix run .#acme-dns-hook-namecheap-dispatch`. It maps names under
  surmount.systems to `~/.local/share/surmount/issue-le-prod/namecheap.env`
  (or `$XDG_DATA_HOME/...`) and other zones to
  `~/.local/share/surmount/namecheap/<sld.tld>.env` (regular file, mode
  0600), then execs `nix run .#acme-dns-hook-namecheap-bin`. Extra-zone TXT
  is never written through the surmount.systems env. Production issue is
  `--issue --directory production` (Let's Encrypt production). Do **not**
  put unowned, archived, or still-Cloudflare zones on `--domains` until
  their nameservers are Namecheap. Extra certificate hostnames belong
  on a leaf only after that NS cut. Host in-process ACME stays off.
- **MTA-STS policy path:** `GET /.well-known/mta-sts.txt` when `mtaStsMode`
  is `testing` or `enforce` (default **off**). Served only for Host
  `mta-sts.<primaryDomain>` on the allowlist (HTTP/2 uses Host or URI
  `:authority` via `request_authority_host`). **Live (2026-08-12):** production
  leaf includes `mta-sts` and **mail** (plus six extra static zones
  apex+www, cryptoquick apex/www, and `mail.cryptoquick.com`; live **20**
  names). Host-local `mtaStsMode=testing`; public
  policy **200** over HTTP/1.1 and HTTP/2. Stay **testing** while DNS MX
  is eforward. DNS + policy body: [DNS.md](DNS.md).
- **In-process ACME (operator-directed scaffold, 2026-08-10; A1/A2 2026-08-10):**
  optional DNS-01 path inside management-ui via **`instant-acme` 0.8** (rustls
  0.23 / aws-lc-rs). **Default off** (`surmount.managementUi.acme.enable =
  false` / no `SURMOUNT_ACME_ENABLE`). Static host PEM load remains first-class
  (not ACME-only forever).
  - **Reuse / early renew:** reuse PEMs when leaf is not past notAfter, not
    not-yet-valid, certificate hostnames cover configured domains, **and** remaining lifetime
    is at least `SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY` days (scaffold default
    **30**; common LE operator practice, not locked CA law). Else issue and
    write cert+key PEMs to `tlsCertPath`/`tlsKeyPath` with fail-closed key mode
    (0600).
  - **DNS-01 challenge adapter (`DnsProvider` / `SURMOUNT_ACME_DNS_PROVIDER`):**
    this name means the component that creates/deletes `_acme-challenge` TXT
    for CA validation. It is **not** a Surmount commercial DNS product and
    **not** a mandatory third-party vendor. Values: `none` (default; reuse
    PEMs only), `mock` (lab self-signed; refused against production Let's
    Encrypt directory), `external-hook` / `hook` (operator-owned absolute
    **regular-file** executable via `SURMOUNT_ACME_DNS_HOOK`; no symlink;
    not group/world-writable; argv `set|clear|wait`; no shell; cleared child
    env + fixed minimal PATH; timeout 1..600s; no Cloudflare/Route53 crates).
    Product wait retries are brief (long DNS propagation stays in the operator
    hook). Surmount runs on operator-chosen host + operator-controlled DNS;
    external-hook is the first real adapter for that model. When
    `acme.enable` + `dnsProvider = external-hook` and `dnsHookPath` is unset,
    the management-ui module may default `dnsHookPath` to the packaged
    Namecheap helper store path (`pkgs.acme-dns-hook-namecheap`). That package
    is **code** only (`nix run .#acme-dns-hook-namecheap-bin`); Namecheap API
    credentials stay laptop Domain A custody; Domain B activation copy prefers
    durable `/var/lib/surmount/secrets/acme/namecheap.env` (S8); `/run` remains
    allowlisted for optional short-lived installs.
  - **Wildcards refused** until DNS-01 TXT naming is designed. **TLS-ALPN-01
    residual.** Fail closed on issuance failure (no silent cleartext).
    Hot-reload residual: restart after renew. CI never requires live Let's
    Encrypt. Env: `SURMOUNT_ACME_*`. Ops sketch: [OPS.md](OPS.md).
- **Production assumption (operator 2026-08-02):** TCP **port 80 is free** on
  the NixOS box so the product redirect-only listener can bind. That free
  :80 is for **redirect/upgrade only**, not an invent of ACME-on-product-:80.
  Day-one ops: [OPS.md](OPS.md).
- Fixed-window rate limit; when peer is loopback, trust **X-Real-IP only**
  (useful behind dual-run nginx). X-Forwarded-For is ignored for rate-limit
  keys (leftmost XFF is spoofable). Never trust forwarding headers from
  non-loopback. Same IP key feeds the ban/whitelist layer.
- **Vaultwarden path proxy (optional, default off):** when
  `managementUi.vaultwardenProxyEnable` is true, management-ui reverse-
  proxies public prefix `/vault` (env `SURMOUNT_VAULTWARDEN_PROXY*=`) to
  loopback Rocket (`http://127.0.0.1:8222` by default). Path strip/rewrite,
  method/headers/body forward, WebSocket Upgrade day-one, 502 when upstream
  down. VW login is SoT on the prefix (no Nostr gate day-one). Align
  `vaultwarden.domain` / console URL to `https://{servicesHostname}/vault`
  when publishing. Not nginx; not a second subdomain. Prefer after free-443
  and public https listen are real. See [SECURITY.md](SECURITY.md).
- **Splora Unix proxy (optional, default off):** `surmount.sploraProxy.enable`
  maps public Hosts to HTTP/1.1 Unix sockets on the **edge host**. Documented
  Hosts (optional edge, not a REST requirement): portal
  `splora.surmount.systems` (GET / lists live non-mainnet Hosts; env
  `SURMOUNT_SPLORA_PORTAL_HOST`; not an indexer),
  `esplora.surmount.systems` to `/run/splora/mainnet.http.sock`,
  `testnet3.esplora.surmount.systems` to `/run/splora/testnet3.http.sock`,
  `testnet4.esplora.surmount.systems` to `/run/splora/testnet4.http.sock`,
  `mutinynet.esplora.surmount.systems` to `/run/splora/mutinynet.http.sock`,
  `liquid.esplora.surmount.systems` to `/run/splora/liquid.http.sock`. The
  portal Host and indexer Hosts join the :80 allowlist and Arti extra onion
  Hosts when the proxy is on. The
  public sample stays proxy off. Private host-local enables the proxy.
  Indexer sockets exist only when those units run. Unix socket or one
  existing Host is enough for mempool REST. Do not map REST onto the
  mail console Host. Five esplora Let's Encrypt names are optional
  edge; they are not on the live leaf today and are not a REST gate.
  The Axum edge forwards **Host** and **X-Forwarded-Proto** (tests fail
  if proto is omitted). `GET /api/v1/ws` WebSocket-upgrades to the same
  indexer HTTP socket. Queue `POST {npub,email}` goes only to
  `/run/splora/queue.sock` on `/splora/queue`, never to indexer units.
  Queue is not REST. NIP-98 stays in splora (no edge API keys). The
  Electrum newline Unix socket is **not** proxied (fail-closed if
  configured). Not nginx; do not enable `modules/web.nix` for this path.
  Backend from the edge is still HTTP/1.1 over UDS when the client arrived
  on HTTP/3. HTTP/3 and hypervisor UDP 443 are optional edge, not
  prerequisites of the mempool REST. When `surmount.sploraProxy.enable`
  and `services.splora.enable` are both true, `users.users.surmount-ui.extraGroups`
  includes the `splora` group so the edge can connect to 0750 sockets.
  Flake input `splora` (`github:SurmountSystems/splora` on the `surmount`
  branch, locked rev `be3603dbd8e6ef0c24e37fe07beaf8067bfa2d0b`) supplies
  `pkgs.splora`, `pkgs.splora-liquid`, and `nixosModules.splora`.
  Upstream crane omits `.cargo/config.toml` from Nix src (laptop cargo
  still uses Menhera). This overlay does not wrap that src again. Do
  not set `services.splora.enable` or `surmount.sploraIndexer.enable` in
  the public sample host. Splora REST **requires a Bitcoin JSON-RPC peer
  by design**. The indexer is not Bitcoin Core. That is **not a bug**.
  The remote shape is `daemonDir = null`, a cookie file path
  (`cookieFile`; never cookie bytes in git), and a JSON-RPC address
  (`daemonRpcAddr`). REST needs that peer plus one indexer instance, not
  a local bitcoind datadir on this guest. Host-local sets
  `surmount.sploraIndexer` (one instance, first-class `--jsonrpc-import`,
  cookie path, `daemonDir = null`, 24 MiB db cache; optional
  `--public-health`). Bitcoin node inventory is tasked in the splora
  tree (branch `surmount`). Do not start five indexers.
- **HTTP/3 (required on HTTPS, not later):** UDP :443 QUIC next to TCP :443,
  same host PEMs, TLS 1.3. That is Axum edge product, not a mempool REST
  prerequisite. Hypervisor UDP 443 is only if clearnet HTTP/3 should
  answer from the public internet. QUIC rustls ALPN is **h3 only**; TCP ALPN stays
  `h2` + `http/1.1`. Same Axum `Router`. Stack: quinn + h3, via
  [axum-h3](https://crates.io/crates/axum-h3) 0.0.6 production **quinn**
  backend (`h3-util` feature `quinn`; accessed: 2026-08-31). **ConnectInfo:**
  TCP HTTPS uses `into_make_service_with_connect_info`. The QUIC path
  (`H3Router::from(app)`) does **not** insert `ConnectInfo<SocketAddr>`.
  A required extractor there is Axum 500 text ("Missing request extension"),
  which the `/login` NIP-07 script shows as **Login failed: 500**. Session
  exchange uses an optional peer (unspecified when missing) and does not
  ban 0.0.0.0/`::`. Leftover: inject the real QUIC peer into `ConnectInfo`
  so H3 bans and rate-limit keys are per-client. Clearnet
  Alt-Svc adds `h3=":443"` **only if** the QUIC listener bound; onion `h2`
  Alt-Svc stays a separate token (merged, not replaced).
  `networking.firewall.allowedUDPPorts` includes 443 when the UI HTTPS
  listener is on (not the cleartext https-escape). Keep the workspace
  `[patch.crates-io]` for `chacha20` if quinn pulls that crate (menhera
  yank of 0.10.0/0.10.1) and for `rustls` 0.23.45 until menhera 10d lists
  that version (RUSTSEC-2026-0285; do not ignore; do not fetch crates.io).
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
- Version assumption: Surmount-owned Arti **2.5.1** TOML shape
  (`nix/packages/arti-onion-service.nix`; GitLab `arti-v2.5.1`), not stock
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
- Residual: live Tor verify; operator offline backup of HS identity;
  hardening in the **public** module after real `arti proxy`; local
  temp-key e2e != operator backup (see RESIDUAL.md). Do **not** claim B3
  fully closed. Package overlay is in-tree.

### Onion-Location and Alt-Svc (Axum edge, 2026-08-17)

Mandatory discovery headers on the custom Rust Axum edge
(`security_headers_middleware` in `surmount-management-ui`). Production
public HTTPS is still **Let's Encrypt production**. This slice does not
change issuance.

- Mapping loads **once at process start** from the onion surface
  (`SURMOUNT_ONION_URL` maps the **services** Host only), plus per-site
  published addresses (`SURMOUNT_ONION_SITE_NICKNAMES_FILE` +
  `SURMOUNT_ONION_PUBLISHED_HOSTNAMES_DIR`, optional
  `SURMOUNT_ONION_SITES_DIR` / `SURMOUNT_ONION_MAP_FILE`), plus
  `SURMOUNT_PRIMARY_DOMAIN`, `SURMOUNT_SERVICES_HOSTNAME`, extra static
  Hosts, and extra mail Hosts to skip. Each public HTTP Host gets its
  own v3 (port 443, `h2`, ma 86400). Mail and unmapped hosts emit
  nothing. Sharing one v3 across Hosts is a correlation leak. No
  hot-reload; restart the unit after hostname file, map file, static
  vhost map, or env change (same as PEMs).
- Emit only when `listen_mode` is https, Host is a mapped clearnet name
  (not `.onion`), status is 2xx/3xx, and the request is not on the `:80`
  redirect router. Local Arti cleartext (`SURMOUNT_LOCAL_CLEARTEXT_LISTEN`)
  does **not** emit these headers.
- Onion-Location: `{scheme}://{that-host-onion}{path}{?query}` (empty
  path -> `/`). Onion-side middleware maps the request `.onion` Host to
  that site's clearnet Host and keeps the path. There is no `/_o/{host}`
  discriminator. Arti publishes one onion service nickname per public
  HTTP Host (`/etc/surmount/arti.toml`, `/etc/surmount/onion-site-nicknames.json`).
  Console keeps `artiHiddenService.nickname`.
- Alt-Svc: `h2="{onion_host}:{port}"; ma={ma}; persist=1`.
  Clearnet HTTP/3 is a **separate** token (`h3=":443"`) added only when
  the QUIC listener is bound. Onion `h2` is not replaced by `h3`.
- Optional env: `SURMOUNT_ONION_LOCATION_ENABLED`,
  `SURMOUNT_ONION_ALT_SVC_ENABLED` (default on),
  `SURMOUNT_ONION_LOCATION_DISABLED_HOSTS`,
  `SURMOUNT_ONION_ALT_SVC_DISABLED_HOSTS` (comma lists),
  `SURMOUNT_ONION_MAP_FILE` (JSON extra/override mappings; missing or
  malformed entries skipped).
- Diagnostic dump: `GET /api/v1/system` field `onion_discovery`
  (admin-gated when Nostr is on). Not on `/health`.
- **Live (2026-08-17):** headers proven with HTTPS curl on 307/200 for
  `surmount.systems`, `www.surmount.systems`, and
  `services.surmount.systems`. Unit `surmount-arti-hidden-service`
  active. That live generation still used one shared v3. This tree
  (2026-09-11) is per-site onions after the next host switch. Tor
  Browser purple pill **BLOCKED** (header presence is not an Alt-Svc
  upgrade proof).
- **Every public HTTP Host (2026-08-20; per-site 2026-09-11):** extra
  static vhosts and `mta-sts.{primary}` emit dual headers at each Host's
  own onion root. `/vault` on the services Host emits path-preserving
  Onion-Location when the Vaultwarden proxy is on (VW login is SoT on
  that prefix; the edge does not Nostr-gate `/vault` once proxy enable is
  true). Six extra static zones (yiffa.app, baxterartworks.com,
  btcfur.com, iantuckerstudios.com, nostrfurs.com, exophiles.org) are on
  the live Let's Encrypt certificate this host presents (apex+www).
  cryptoquick Hosts on this :443 map may exist; they are first-class
  static vhosts like those six. Live certificate still omits cryptoquick
  apex/www (**18** names). Intended certificate is **20** names on one
  PEM. The SERVFAIL A-lookup gate is closed. Mail stays unmapped. Tor
  Browser purple pill still not claimed.

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
| **Certs** | Static host PEMs always supported. **In-process** path uses **instant-acme** (DNS-01, default off). Dual-run may still use host `security.acme`. **Not locked** ACME-only or to a single CA. |
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

Harness SoT: `nix run .#e2e-host` (Rust `crates/surmount-e2e`). Host e2e is never
a default flake check. Expired certs fail those TLS rows.

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
| PQ KEX on Axum/rustls | **First-class (D1):** **aws-lc-rs** + `prefer-post-quantum` (X25519MLKEM768 first in default provider). Hermetic unit tests pin group config. **Live host hybrid negotiation is residual** until measured post-B1 (do not claim from local e2e alone). Probe: `nix run .#surmount-tls-hybrid` / `just check-tls-hybrid` (requires `SURMOUNT_E2E_BASE_URL`; exit 2 BLOCKED if unset) |
| OpenSSL 3 | Host hybrid probe + other host tools / some mail stacks; not the default in-process edge |
| PQConnect | **Separate** E2EE PQC path layer (**D2**); research + human-owned sibling packaging; does not replace D1 TLS hybrid KEX |
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
`pkgs.artiOnionService`. Clearnet browsers learn the onion via
**Onion-Location** and **Alt-Svc** on the Axum HTTPS edge (see above).
**Live (2026-08-17):** unit `surmount-arti-hidden-service` active; hostname
file present; discovery headers proven on mapped HTTPS. Residual: live Tor
Browser / onion-fetch verify, operator offline HS backup, public-module HOME.
Local optional Tor row: `just e2e` (temp keys; not operator backup). Do
**not** claim B3 fully closed.

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
| 80 | Product redirect-only (default path); dual-run ACME if nginx escape | Yes (Axum / dual-run) |
| 443 | Product HTTPS (management-ui rustls) | Yes (Axum; **not** Stalwart product edge) |
| 25/465/587/993/4190 | Mail | No (Stalwart) |
| UI loopback / Stalwart HTTP :8080 | Local only | Loopback; free Stalwart public HTTPS via nix/stalwart plan |

## Open questions

**Q-EDGE-1.** Shared host ACME PEM files for mail + web vs in-process issuance
on the Axum edge for web only? **Status (2026-08-10):** in-process ACME for
management-ui web is **operator-directed work in progress / scaffold**
(default off, DNS-01, instant-acme). Does **not** close shared mail+web PEM
pipeline, multi-service renew, or full Q-EDGE. Do not claim all edge Q-* closed.

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
