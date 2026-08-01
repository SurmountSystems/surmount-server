# Research: Rust-first HTTPS edge and Unix sockets

**Status:** research finding. Not operator acceptance of every engine below.
**Date:** 2026-07-30
**Scope:** practical options to **replace nginx** on Surmount with a Rust-first
HTTPS edge, cert automation, rate limits, and Unix domain sockets (UDS) to
backends where possible.

## Operator preference (2026-07-30 follow-up)

**Axum-first is the product preference.** Prefer a first-party Axum (or
Axum + Leptos / small same-workspace `surmount-edge`) HTTPS edge over a
separate reverse-proxy product (Caddy, nginx, Sozu, etc.) unless measured
need appears. Edge work is still real (TLS, certs, rate limits, routing,
headers) but should live in-process or in-workspace, not as a third-party
proxy daemon by default.

Living product notes: [EDGE_AND_TLS.md](../EDGE_AND_TLS.md),
[open-choices.md](../open-choices.md),
[operator-direction.md](../operator-direction.md).
TLS trust / ACME not locked: [tls-trust-and-acme.md](tls-trust-and-acme.md).

This file remains the deeper comparison (UDS evidence, non-default
candidates). Treat non-Axum rows as optional later research, not competing
defaults.

**Scaffold today:** `modules/web.nix` runs **nginx** + `security.acme`. Treat
that module as **replace**, not permanent product identity.

---

## Requirements (operator)

| Need | Notes |
|------|-------|
| No nginx | Security posture / maintenance taste; leave C codebase at the public edge |
| Prefer Rust + middleware | Align with Axum/Leptos product stack; tower-style limits |
| ACME | Let's Encrypt (or compatible); HTTP-01 and/or TLS-ALPN-01 |
| Rate limiting | Edge and/or app; defense in depth with Stalwart mail anti-abuse |
| UDS to backends | Prefer Unix sockets to management UI and Stalwart HTTP if possible |
| Public 80/443 | Standard Internet ports; no required Cloudflare hop |
| Internal via UDS (or loopback) | Backends not on the public interface |
| NixOS-friendly | Prefer real modules or small flake packages + systemd |

Mail protocol ports (25/465/587/993/4190) stay on **Stalwart**, not the edge.
See [STACK.md](../STACK.md).

---

## Current scaffold (what to replace)

| Piece | Today | Target shape |
|-------|-------|--------------|
| Public HTTPS | `services.nginx` in `modules/web.nix` | Non-nginx edge binary + module |
| ACME | NixOS `security.acme` (HTTP-01) for nginx vhosts | Edge-native ACME **or** shared cert files under `/var/lib/acme` |
| Management UI | TCP `listenAddress:port` (loopback), env `SURMOUNT_LISTEN` | Prefer UDS under e.g. `/run/surmount/ui.sock` |
| Stalwart HTTP | First-boot often `:8080` (may bind broadly until rebind) | Loopback TCP for now; UDS **if/when** engine supports it |
| Static / legacy vhosts | nginx `extraVhosts` / redirects | Edge static file server + redirects |
| Rate limit | Mild body size; fail2ban sketch; no strong edge `limit_req` story | tower/governor or proxy-native limits + Axum API limits |

`modules/management-ui.nix` already allows `AF_UNIX` in
`RestrictAddressFamilies`, so UDS for the UI is a small product change, not a
systemd fight.

---

## Stalwart 0.16.15: can HTTP bind a Unix socket?

**Finding: no, not in the documented 0.16 listener model.**

Evidence (official docs, current as of this research):

1. [Listeners overview](https://stalw.art/docs/server/listener/overview):
   `bind` is a list of **`host:port`** entries (IPv4/IPv6). Examples are
   `[::]:25`, `127.0.0.1:8080`, etc.
2. [NetworkListener ref](https://stalw.art/docs/ref/object/network-listener):
   field `bind` type is **`SocketAddr[]`**. In Rust, `std::net::SocketAddr`
   is TCP/UDP IP addresses only. Unix paths use a different type
   (`std::os::unix::net::SocketAddr` / tokio UDS), not this field.
3. Socket knobs on the object are TCP-oriented (`socketNoDelay`,
   `socketReusePort`, TOS, TTL, buffer sizes).
4. Community signal: operators have asked for a Unix socket to manage
   Stalwart when locked out of the WebUI
   ([support thread](https://support.stalw.art/t/i-locked-myself-out-of-the-stalwart-web-ui/352));
   that wish implies UDS is not a current first-class path.

**Surmount pin:** server **0.16.15** (see
[stalwart-0.16.15-stores-evidence.md](stalwart-0.16.15-stores-evidence.md)).
Nothing in our packaging rewrites listener types.

### Practical backend story for Stalwart HTTP

| Approach | Status |
|----------|--------|
| UDS path like `/run/stalwart/http.sock` | **Not supported** by documented NetworkListener |
| `127.0.0.1:8080` (or other loopback port) | **Supported**; rebind from first-boot defaults |
| Firewall / no public 8080 | **Required** regardless of edge choice |
| PROXY protocol from edge | Stalwart has `overrideProxyTrustedNetworks` for proxy protocol on listeners; useful if edge sits in front of TCP loopback |

Until upstream adds UDS bind (or Surmount proves otherwise against source for a
newer tag), **edge -> Stalwart stays loopback TCP**. That is still a solid
posture if the port is loopback-only and firewalled.

---

## Candidate comparison

Ratings are relative to **one NixOS mail VPS**, few vhosts, single operator.
Not Cloudflare scale.

Legend: **Y** yes / **P** partial / **N** no or not serious yet / **?** unverified.

### Summary table

| Candidate | Kind | ACME | Rate limit | Upstream UDS | NixOS module | Maintenance | Fit for Surmount |
|-----------|------|------|------------|--------------|--------------|-------------|------------------|
| Custom Axum/hyper + rustls-acme | In-tree binary | Y (crate) | Y (tower) | Y (we own it) | Custom flake module | We own it | **Strong near-term Rust path** |
| tower middleware only | App layer | N (not edge) | Y | N/A | N/A | Low | **Always**, under any edge |
| Pingora | Framework | N (build yourself) | P (limits crates) | Y (framework) | N | High eng cost | Long-term / specialized |
| River (on Pingora) | App binary | ? incomplete | ? | ? | N | Paused / unstable | **Not ready** |
| rama | Framework + CLI | Y (`rama-tls-acme`) | P (compose) | Y (`rama-unix`) | N | Growing; young API | **Strong longer-term** |
| Sōzu | Product proxy | P (certs + helpers) | P | P (command UDS; check backends) | Package maybe; thin service story | Production at Clever Cloud; **AGPL-3.0** | Serious off-the-shelf Rust |
| rpxy | Product proxy | Y (TLS-ALPN-01) | P | ? primarily host:port | N | WIP, single maintainer | Watchlist |
| Taxy | Product proxy + WebUI | Y (HTTP-01) | ? | ? | N | Smaller project | Watchlist |
| Caddy / Traefik | Not Rust-first | Y | P | Y (Caddy `unix//path`) | Y (Caddy excellent) | Low | Escape hatch only here |

### 1. Custom Axum / hyper edge + rustls-acme / instant-acme

**What it is:** a small Surmount-owned binary (or a split crate next to
`management-ui`) that:

- binds `:80` / `:443`
- terminates TLS with **rustls**
- obtains/renews certs via **rustls-acme** and/or **instant-acme**
- reverse-proxies by Host/path to backends
- applies **tower** middleware (timeouts, body limits, **tower-governor** or
  equivalent)
- serves static files and redirects for apex/www/mail name landing

**Building blocks (versions known at research time):**

| Crate | Version / note | Role |
|-------|----------------|------|
| `axum` / `hyper` / `hyper-util` | Current 1.x hyper ecosystem | HTTP server and client proxy |
| `rustls` + `tokio-rustls` | Current | TLS terminate |
| `rustls-acme` | **0.15.3** (docs.rs, 2026-06-05) | TLS-ALPN-01 and HTTP-01; optional axum helpers; DirCache |
| `instant-acme` | Mature low-level ACME client (used by many tools) | Order/account control if not using rustls-acme high-level API |
| `tower-governor` (or `tower` limit layers) | Common Axum rate-limit stack | Per-IP / per-route limits |
| `hyperlocal` / tokio `UnixStream` | Standard pattern | Upstream UDS to management UI |
| `tower-http` | Compression, trace, set header | Middleware |

**Pros**

- Matches product language and skills (same team as management UI).
- Full control of ACME challenge mode, multi-vhost, WebSocket upgrade to UI.
- UDS upstream is straightforward: we implement the connector.
- Rate limits and auth-adjacent middleware can share patterns with Axum app.
- NixOS: one flake package + one systemd unit + small options module (same
  pattern as `management-ui.nix`).
- Can still **consume** certs from disk if we ever share material with Stalwart
  mail TLS file paths (export PEMs on renew).

**Cons**

- **We own** ACME failure modes, renewal races, multi-name SAN/issuance,
  HTTP-01 vs ALPN, certificate permissions, and security updates of the edge
  itself.
- Not a decade of reverse-proxy corner cases (chunked edge cases, weird
  clients). For a few vhosts this is usually fine; do not claim "nginx parity."
- Need explicit design for: graceful reload, log format, metrics, static sites,
  optional `/stalwart-admin/` path proxy.
- No upstream NixOS module to lean on.

**Verdict:** best **pragmatic near-term Rust-first** path for Surmount if the
operator accepts owning a thin edge crate. Scope the first cut to: ACME +
vhosts + proxy + limits + static/redirects. Do not boil the ocean.

### 2. tower middleware (governor, limits) -- app layer, not a full edge

**What it is:** rate limit, concurrency, timeout, request body caps on the
**Axum management UI** (and later webmail APIs).

**Pros**

- Low cost; works whether edge is custom, Sōzu, or temporary nginx.
- Correct place for auth-sensitive throttles (`/api/*`, Nostr login).
- Already the natural stack for Leptos/Axum.

**Cons**

- Does **not** replace TLS termination, ACME, multi-vhost, or protecting
  Stalwart HTTP.
- Edge-level connection floods still need an outer limit.

**Verdict:** **always do this**, independent of edge engine. Not a substitute
for nginx replacement.

### 3. Pingora (Cloudflare open source)

**What it is:** Rust **framework** to build proxies and network services.
Apache-2.0. Battle-tested inside Cloudflare (very high RPS). Open-sourced
2024; still actively maintained as a library (MSRV ~1.85 per upstream README).

**Highlights**

- HTTP/1 and HTTP/2 end-to-end proxy; gRPC and WebSocket.
- TLS: OpenSSL, BoringSSL, s2n-tls, rustls (**experimental**).
- Explicit support for building on TCP **or UDS** in the framework narrative.
- `pingora-limits` and related crates for counting / load shedding.
- Graceful reload, load balancing primitives.

**Gaps for Surmount**

- **Not** a drop-in reverse proxy with a Caddyfile. You write a Rust program.
- **No first-class ACME product** in the core story; certificate automation is
  your problem (or River's, if it existed as a finished app).
- Build deps can pull Clang/Perl and OpenSSL/BoringSSL toolchains; heavier than
  pure rustls stacks on Nix.
- No NixOS `services.pingora` module in the usual "flip enable = true" sense.
- Ops model is "we maintain a custom proxy binary," same class as custom Axum
  but with a larger, CF-shaped API surface.

**Verdict:** excellent **library** if Surmount later needs exotic L7 behavior
at scale. **Poor near-term** choice vs thin Axum edge or a real product proxy.
Do not pick Pingora just to say "we use Cloudflare's nginx killer."

### 4. River (ISRG / Prossimo, on Pingora)

**What it was meant to be:** batteries-included reverse proxy on Pingora
(memory-safety push with Cloudflare, Shopify, Chainguard).

**State (research time):** repo `memorysafety/river`, about **v0.5.0**,
README: **"Until further notice, there is no expectation of stability."**
Config via KDL; still demonstration-grade. Community reports development
paused or slow relative to the 2024 announcement.

**Verdict:** **not a Surmount dependency.** Re-check only if River ships a
stable release with ACME and packaging.

### 5. rama (plabayo)

**What it is:** modular Tokio-native **service framework** for clients,
servers, and proxies. Explicit stacks (transport -> TLS -> HTTP -> middleware).
MIT/Apache-2.0. Active development (thousands of commits). MSRV **1.96** (new).

**Relevant crates**

| Crate | Role |
|-------|------|
| `rama-tls-acme` | ACME |
| `rama-tls-rustls` / boring | TLS |
| `rama-unix` | Unix domain sockets |
| `rama-proxy` / examples | Reverse and other proxies |
| `rama-http` / `rama-http-backend` | HTTP services (Hyper-based internals) |
| `rama-tower` | Tower interop |
| `rama` CLI | Run some stacks without writing full apps |

**Pros**

- Designed for programmable proxies and gateways, not only "HTTP hello."
- First-class UDS and ACME modules in the monorepo.
- Production claims (security analysis, API gateways); commercial support
  exists via Plabayo.
- Longer-term could own both edge and advanced routing in one framework.

**Cons**

- Still a **framework**: Surmount would compose and own the binary (unless CLI
  covers enough, which is unlikely for full multi-vhost mail-host edge).
- Young/fast-moving API; MSRV may outpace conservative nixpkgs rustc unless
  we use a newer toolchain (acceptable for greenfield, but plan for it).
- No NixOS module; smaller ops community than Caddy/nginx.
- Learning cost higher than "Axum reverse_proxy handler + rustls-acme."

**Verdict:** best **longer-term framework** if Surmount wants a serious Rust
network platform beyond a thin edge. Not the fastest path to delete nginx next
week unless someone already knows rama.

### 6. Sōzu (sozu-proxy) -- serious off-the-shelf Rust proxy

**What it is:** production HTTP reverse proxy in Rust, used by **Clever Cloud**
as their edge. Dynamic config over a **command Unix socket**, hot workers,
designed not to drop connections on reconfigure. **Sōzu 2.0** announced 2026
(HTTP/2 multiplexer and large rewrite); crates.io showed **2.2.x** class
releases in mid-2026.

**Pros**

- Real product proxy, not a tutorial framework.
- Rust memory-safety story without writing our own accept loop.
- Runtime reconfigure fits immutable infra stories.
- ACME-related tooling exists historically (`sozu-acme`); 2.x improved cert
  fullchain loading. Can also load PEMs from NixOS `security.acme`.
- Linux-focused; systemd-friendly packaging narrative from upstream.

**Cons**

- License: **AGPL-3.0** (main binary). Fine for self-host if acceptable to
  operator; may be a hard no for some product distribution stories. Confirm
  before adopting.
- Mental model is **control-plane + workers**, not a 20-line Caddyfile.
- NixOS: may exist as a package in some channels; **not** as mature as
  `services.nginx` / `services.caddy`. Expect a custom module.
- Rate limiting and "boring multi-vhost + static files" need verification in
  2.x docs for our exact features.
- Backend UDS: command plane is UDS; **application backend UDS** must be
  confirmed in 2.x config before promising it. Loopback TCP is the safe bet.

**Verdict:** strongest **buy-not-build** Rust proxy if AGPL is OK and we want
less ACME/proxy code in-tree. Still more ops novelty on NixOS than Caddy.

### 7. rpxy (junkurihara/rust-rpxy)

**What it is:** hyper + rustls + tokio reverse proxy. Multi-domain TLS,
HTTP/1.1/2, experimental HTTP/3, TLS-ALPN-01 ACME via **rustls-acme**. TOML
config. Claims some production use; README still says **work-in-progress**.

**Pros**

- Feature-rich for a single binary (ACME, path routing, PROXY protocol in).
- Pure-ish Rust crypto path (aws-lc-rs).
- Close to what a custom edge would reinvent.

**Cons**

- WIP label; single-maintainer risk; breaking config churn.
- No NixOS module.
- Upstream locations documented mainly as `host:port`; UDS not a headline
  feature.
- ACME marked experimental historically (improving, but own the risk).

**Verdict:** **watchlist**. Prefer in-tree edge (we control) or Sōzu
(production vendor) over depending on rpxy for a mail host.

### 8. Taxy and other small Rust proxies

**Taxy:** reverse proxy with built-in WebUI; tokio/hyper; Let's Encrypt
HTTP-01. Smaller community than Sōzu; treat as experimental for mail-host
critical path.

**Others skimmed and not serious for Surmount today:** ad-hoc "build a load
balancer in 50 lines of Pingora" blogs; huginn-style fingerprinting proxies;
L4-only toys. **actix** can terminate TLS and proxy, but there is no
actix-based edge product that beats Axum+rustls-acme for our stack, and we
are already on Tokio/Axum for the UI.

### 9. Not Rust-first (recorded only)

| Tool | Why mentioned | Why not default in *this* research |
|------|---------------|-------------------------------------|
| **Caddy** | Best NixOS ACME+proxy ergonomics; UDS upstream `unix//path`; prior EDGE_AND_TLS target | Go, not Rust-first |
| **Traefik** | Strong ACME; k8s DNA | Overkill; not Rust |
| **HAProxy** | Excellent proxy; UDS backends | C; ACME external |
| **nginx** | Current scaffold | Explicitly leaving |

If Rust-first is abandoned for schedule risk, Caddy remains the boring
non-nginx cutover. That choice lives in EDGE_AND_TLS; this document does not
re-litigate it except as fallback.

---

## UDS design sketch (when edge is Rust)

```text
Internet
   |
   | :80  ACME HTTP-01 + redirect
   | :443 HTTPS (rustls)
   v
+---------------------------+
| surmount-edge (Rust)      |
| tower limits + vhost map  |
+-------------+-------------+
              |
     +--------+------------------+
     |                           |
     v                           v
 unix:/run/surmount/ui.sock   http://127.0.0.1:8080
 management-ui (Axum)         Stalwart HTTP (JMAP/admin)
                              until upstream UDS exists
```

| Backend | Transport | Notes |
|---------|-----------|-------|
| management-ui | **UDS** | Change `SURMOUNT_LISTEN` (or new env) to socket path; socket dir owned by edge+ui group; mode `0660` |
| Stalwart HTTP | **Loopback TCP** | Rebind listener to `127.0.0.1:8080`; optional PROXY protocol if we need client IP |
| Vaultwarden (later) | UDS or loopback | Same pattern as UI |
| Static legacy sites | Files on edge | No app server |

Socket filesystem hygiene:

- RuntimeDir via systemd (`/run/surmount/`)
- Separate units; `Requires=` + `After=` ordering
- Do not put sockets on durable disk unless necessary

---

## ACME and mail TLS interaction

Edge ACME solves **HTTPS vhosts**. Mail TLS (465/993/STARTTLS) is **Stalwart**
reading cert files (or its own ACME, if enabled upstream). Prefer one of:

1. **NixOS `security.acme`** remains source of PEMs; edge and Stalwart both
   read `/var/lib/acme/<name>/` (works with nginx today; works with any edge
   that can load files instead of internal-only storage).
2. **Edge-native ACME** (rustls-acme DirCache) **exports** or dual-writes PEMs
   Stalwart can read (more glue).
3. Stalwart-native ACME for mail names only; edge ACME for `services.` UI name
   (two issuers; more moving parts).

For a first Rust edge cutover, **(1) file-based certs from `security.acme`**
or **edge ALPN with explicit PEM export** beats trapping certs in an opaque
cache with no mail-path story.

---

## Rate limiting posture (unchanged layers)

1. **Edge:** per-IP request/connection limits; body size cap (UI is not the
   mail blob path).
2. **App (Axum):** tower-governor on auth and API routes.
3. **Stalwart:** engine anti-abuse for mail.
4. **fail2ban / SSH:** host layer; HTTPS jails only with clean logs.

---

## NixOS packaging notes

| Approach | Work |
|----------|------|
| Custom edge crate | `nix/packages/surmount-edge.nix` (crane) + `modules/edge.nix` (or rewrite `web.nix`) + systemd |
| Sōzu | Package pin + custom module + cert path wiring; confirm AGPL |
| rama / Pingora | Same as custom binary; heavier deps for Pingora SSL |
| Remove nginx | `services.nginx.enable = false`; free 80/443; update STACK, EDGE_AND_TLS, options descriptions |

Mark **`modules/web.nix` as replace**: do not add features to nginx beyond
keep-the-lights-on. New vhosts should be designed against the future edge
options shape.

---

## Recommendations

### Near-term (pragmatic, Rust-first)

**Build a thin in-tree edge** (`surmount-edge` or equivalent):

1. hyper/Axum accept on 80/443.
2. TLS via rustls; ACME via **rustls-acme** (TLS-ALPN-01 and/or HTTP-01) **or**
   load PEMs from existing `security.acme` for the first milestone (smaller
   risk).
3. tower middleware: body limit, timeout, **governor** per IP.
4. Reverse proxy:
   - `/` on `services.` hostname -> **UDS** management-ui
   - optional `/stalwart-admin/` -> Stalwart **127.0.0.1:8080**
5. Redirects for apex/www/mail hostname landing (mirror current nginx vhosts).
6. Static file root hook for legacy sites (even if empty at first).
7. NixOS module replaces `services.nginx` in `modules/web.nix`.
8. App-layer tower limits on management-ui in the same effort tranche.

**Why not Sōzu first:** AGPL + control-plane learning + weaker NixOS module
story. Revisit if owning ACME/proxy code is rejected.

**Why not rama/Pingora first:** framework tax before first green deploy.

**Bridge:** keep nginx only until the Rust edge serves the same vhosts in a
VM test; then delete nginx config paths.

### Longer-term

| Path | When to take it |
|------|-----------------|
| **Deepen custom edge** | Default if thin edge stays small and boring |
| **rama** | If we need programmable proxy/gateway features, shared client stacks, or advanced TLS/proxy protocol work beyond thin reverse_proxy |
| **Pingora** | Only for specialized L7 at scale or if River (or another Pingora app) becomes a stable product |
| **Sōzu** | If we decide buy-not-build and accept AGPL + custom NixOS module |
| **Stalwart UDS** | Track upstream; adopt if `bind` gains path sockets; until then loopback TCP |

### Explicit non-goals for the near-term edge

- HTTP/3 on day one
- Multi-node load balancing
- Full WAF
- Replacing Stalwart mail ports
- Depending on Cloudflare orange-cloud

---

## Suggested implementation order (when coding)

1. Red tests / contracts: vhost map, ACME file load or staging ACME, proxy to
   UI over UDS, loopback to Stalwart.
2. management-ui: listen on UDS; systemd socket or path in `/run/surmount`.
3. Edge binary + module flag `surmount.web.backend = "nginx" | "rust-edge"`.
4. Staging rebuild; validate HTTP-01/ALPN renew.
5. Remove nginx; update EDGE_AND_TLS, STACK, options, architecture-review.
6. Harden rate limits; drop optional stalwart-admin path when product UI covers
   ops.

---

## Sources (primary)

- Stalwart listeners: https://stalw.art/docs/server/listener/overview
- Stalwart NetworkListener: https://stalw.art/docs/ref/object/network-listener
- rustls-acme 0.15.3: https://docs.rs/rustls-acme
- Pingora: https://github.com/cloudflare/pingora
- River: https://github.com/memorysafety/river
- rama: https://github.com/plabayo/rama / https://ramaproxy.org
- Sōzu: https://github.com/sozu-proxy/sozu / https://www.sozu.io
- rpxy: https://github.com/junkurihara/rust-rpxy
- In-tree: `modules/web.nix`, `modules/management-ui.nix`, `docs/EDGE_AND_TLS.md`

---

## Open questions for the operator (not decided here)

1. Accept **owning** a thin Rust edge binary, or prefer **Sōzu (AGPL)** /
   temporary **Caddy (not Rust)** for faster nginx deletion?
2. First milestone ACME: keep **`security.acme` PEM files** vs full
   **rustls-acme** in-process?
3. Is AGPL on the public edge binary acceptable for Surmount?
4. Priority of **UDS for UI** vs "loopback TCP is enough for v1"?

Nothing above is locked until the operator says so in writing.
