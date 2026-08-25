# Edge, TLS, ACME, and rate limiting

How HTTPS reaches the management UI and (optionally) Stalwart HTTP. Mail
protocol ports are **not** edge-proxied; they terminate on Stalwart. See
[STACK.md](STACK.md) for the full path map.

**Last updated:** 2026-08-25 (operator bins are `nix run .#...`.) Prior 2026-08-24 (`just deploy` publishes static sites; `just deploy-host` is the NixOS generation.) Prior 2026-08-21 (live production leaf is 18 certificate
hostnames: six extra static zones apex+www plus surmount apex, www,
mail, services, mta-sts, plus **mail.cryptoquick.com** for IMAP/SMTP.
Extra-vhost HTTPS live for those six. `cryptoquick.com` apex/www stay
off the leaf. Prior 2026-08-18: laptop Let's Encrypt renew is
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
| Routing | UI vs optional Stalwart HTTP vs static legacy vs optional Vaultwarden `/vault/` |
| Headers / hardening | baseline security headers, Onion-Location + Alt-Svc on mapped HTTPS, body limits |
| Local upstreams | Prefer UDS; loopback TCP until backends support UDS |
| Vaultwarden subpath | Optional Axum reverse-proxy of loopback Rocket under `/vault/` (no nginx, no new subdomain) |

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
| Public :443 | management-ui rustls (`listenMode=https` + host PEMs or in-process ACME) | Product browser / services HTTPS |
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
  `/var/lib/surmount/secrets/tls/{cert,key}.pem` (mode **0640**, owner
  `surmount-ui`, group `surmount-tls`, not world-readable) and account JSON
  under `.../acme/account.json` (mode 0600). Ephemeral
  `/run/surmount-secrets/...` remains a valid override (wiped on reboot).
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
- **Mail-plane TLS (shared PEMs):** Stalwart does **not** take IMAP/SMTP
  certs from Nix `settings` (that option is ignored). Point the engine at
  the same durable Axum PEMs with `just point-stalwart-mail-tls` (default
  dry-run; `--live --restart` on the host). Stalwart 0.16.15 query JSON is
  certificate hostnames plus `id` (not `filePath`); the driver resolves
  the Let's Encrypt File leaf that way. Nix grants
  `ReadOnlyPaths=/var/lib/surmount/secrets/tls` and group `surmount-tls`
  (key mode **0640**, not world-readable). Template:
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
  **auto-map** Onion-Location / Alt-Svc to the same v3 onion (2026-08-20).
  Onion-Location for extra Hosts, apex/www, and `mta-sts.{primary}` uses
  `/_o/{clearnet-host}{path}` so Tor Browser lands on that surface, not
  the services console. `http://{onion}/` stays the console. Mail stays
  unmapped. Do not put nginx back as product edge.

  **Live in browsers (2026-08-20):** Namecheap NS, exclusive A matching
  this host, HTTPS **200**, production leaf covers apex+www, packaged
  site content, not console, not COMING SOON:
  yiffa.app, baxterartworks.com, btcfur.com, iantuckerstudios.com,
  nostrfurs.com, exophiles.org (each apex + www). Host-local roots under
  `/var/lib/surmount/static-sites/<slug>`. `baxterartworks.com` is also a
  local Stalwart Domain; public MX still parked. Static-site-only extras
  are **not** mail domains.

  **Not live in browsers:** cryptoquick.com + www. Leftover parent DS
  (key tag 2368, alg 13, digest type 1) with no child DNSKEY; validating
  A SERVFAIL. Host tree is populated; insecure `-k` HTTP 200 packaged
  site titled Hunter Beast. Live leaf does **not** include these names
  (intentional). Operator click still required: Namecheap Domain List,
  Manage cryptoquick.com, Advanced DNS, DNSSEC, toggle off. Do not
  re-add key tag 2368. In-tree tools cannot delete DS.

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
  add a name that is already on that leaf. **Live 2026-08-21:**
  `mail.cryptoquick.com` is on this shared production leaf (IMAP :993 /
  SMTPS :465 identity). Do **not** add `cryptoquick.com` /
  `www.cryptoquick.com` unless a later slice proves those web names are
  required. Extra-zone DNS-01 uses the dispatcher hook; the laptop
  `--issue` wrap must re-export `HOME` (ACME `env_clear`) so dispatch
  can find `namecheap/<sld.tld>.env`.

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
  apex+www and `mail.cryptoquick.com`; 18 names total). Host-local `mtaStsMode=testing`; public
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
- Residual: live Tor verify; operator offline backup of HS identity;
  hardening in the **public** module after real `arti proxy`; local
  temp-key e2e != operator backup (see RESIDUAL.md). Do **not** claim B3
  fully closed. Package overlay is in-tree.

### Onion-Location and Alt-Svc (Axum edge, 2026-08-17)

Mandatory discovery headers on the custom Rust Axum edge
(`security_headers_middleware` in `surmount-management-ui`). Production
public HTTPS is still **Let's Encrypt production**. This slice does not
change issuance.

- Mapping loads **once at process start** from the existing onion surface
  (`SURMOUNT_ONION_URL` / `SURMOUNT_ONION_HOSTNAME_FILE` /
  `SURMOUNT_ONION_HS_STATE_DIR` plus `SURMOUNT_PRIMARY_DOMAIN`,
  `SURMOUNT_SERVICES_HOSTNAME`, and extra static Hosts from
  `SURMOUNT_STATIC_VHOSTS` / `_FILE`). Auto-derive apex, `www.{apex}`,
  services, `mta-sts.{apex}`, and extra static Hosts to the **same** v3
  onion (port 443, `h2`, ma 86400). Mail and unmapped hosts emit nothing.
  No hot-reload; restart the unit after hostname file, map file, static
  vhost map, or env change (same as PEMs).
- Emit only when `listen_mode` is https, Host is a mapped clearnet name
  (not `.onion`), status is 2xx/3xx, and the request is not on the `:80`
  redirect router. Local Arti cleartext (`SURMOUNT_LOCAL_CLEARTEXT_LISTEN`)
  does **not** emit these headers.
- Onion-Location: services console `{scheme}://{onion_host}{path}{?query}`
  (empty path -> `/`). Other mapped Hosts:
  `{scheme}://{onion_host}/_o/{clearnet-host}{path}{?query}`. Onion-side
  middleware treats `/_o/{mapped-host}` as that clearnet Host and strips
  the prefix (Axum `Router::layer` runs after routing; extra vhosts and
  MTA-STS policy are served from that rewritten Host, not a second onion
  key). `http://{onion}/` stays the services console.
- Alt-Svc: `h2="{onion_host}:{port}"; ma={ma}; persist=1`.
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
  `services.surmount.systems` (same onion). Unit
  `surmount-arti-hidden-service` active. Tor Browser purple pill
  **BLOCKED** (header presence is not an Alt-Svc upgrade proof).
- **Every public HTTP Host (2026-08-20):** extra static vhosts and
  `mta-sts.{primary}` auto-map the same dual headers. `/vault` on the
  services Host emits path-preserving Onion-Location when the Vaultwarden
  proxy is on (VW login is SoT on that prefix; the edge does not
  Nostr-gate `/vault` once proxy enable is true). Six extra static zones
  (yiffa.app, baxterartworks.com, btcfur.com, iantuckerstudios.com,
  nostrfurs.com, exophiles.org) are on the live Let's Encrypt production
  leaf (apex+www). cryptoquick Hosts on this :443 map may exist; they are
  **not** added to the leaf while leftover parent DS SERVFAILs. Mail stays
  unmapped. Tor Browser purple pill still not claimed.

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
