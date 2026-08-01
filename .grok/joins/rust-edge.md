# Join: Rust-first HTTPS edge + UDS research

**Date:** 2026-07-30
**Artifact:** `docs/research/rust-edge-and-uds.md`
**Scope:** replace nginx; Rust + middleware; ACME; rate limit; UDS backends;
NixOS-friendly. Docs only.

## Stalwart 0.16.15 Unix sockets

**No.** `NetworkListener.bind` is `SocketAddr[]` (IP host:port only). Docs and
ref object are TCP-oriented. Edge -> Stalwart HTTP stays **loopback TCP**
(e.g. `127.0.0.1:8080`) until upstream adds path binds. Rebind + firewall
still required.

## Candidates (honest one-liners)

| Option | Take |
|--------|------|
| Custom Axum/hyper + rustls-acme (~0.15.3) + tower-governor | **Near-term pick** if we own a thin edge |
| tower limits on UI | Always; not an edge substitute |
| Pingora | Framework; no ACME product; high cost |
| River (Pingora app) | Unstable / not ready |
| rama (+ rama-tls-acme, rama-unix) | **Longer-term** framework if edge grows |
| Sōzu 2.x | Serious Rust product proxy; **AGPL-3.0**; buy-not-build alt |
| rpxy / Taxy | Watchlist / smaller |
| Caddy / Traefik | Not Rust-first; escape hatch only in this doc |

## Recommendations

- **Near-term:** in-tree `surmount-edge` (rustls + ACME or `security.acme` PEMs,
  tower limits, proxy UI over **UDS**, Stalwart over loopback). Rewrite
  `modules/web.nix` off nginx.
- **Longer-term:** deepen custom edge, or rama if programmable proxy needs
  appear; Sōzu if AGPL + buy-not-build wins; Pingora/River only if productized.
- **Mark replace:** `modules/web.nix` nginx is transitional; do not grow it.

## Scaffold hooks already friendly

- `management-ui.nix` allows `AF_UNIX`.
- UI listen is env `SURMOUNT_LISTEN` today (TCP); extend for socket path.

## Operator open questions

Own thin edge vs Sōzu(AGPL) vs Caddy fallback? ACME via PEMs first vs
in-process rustls-acme? UDS-for-UI priority vs loopback-enough?

## Not done

No code, no module cutover, no git commit. Living EDGE_AND_TLS still says
Caddy target; this research is the Rust-first deep dive alongside it.
