# Research: merciless access control (rate limit, ban, whitelist)

**Status:** design sketch + **first product path landed** (2026-07-30). Not full
policy (Q-ACL-* still open). Not live host ban proof.
**Date:** 2026-07-30
**Operator direction (this dump):** merciless blacklist on unauthorized access;
explicit whitelist; last-used tracking; IP rate limits; evaluate fail2ban vs
nft vs custom Rust.

### First path in tree (honest)

| Piece | Where | Default |
|-------|--------|---------|
| Decide layer | `crates/management-ui/src/ban.rs` | enforcement **off** |
| Memory + optional JSON state | same | no state path unless configured |
| nft argv builder + `NftExec` / `BanNftApply` | same; tests use `RecordingNftExec` | no process exec in CI |
| Least-privilege helper | `surmount-nft-ban-helper` + UDS client | scaffold; socket oneshot |
| Nix options | `surmount.accessControl.*` | `enable = false`; `nftHelper = false` |
| nft sets | `hardening.nix` when enable+nftSets | empty `surmount-ban4/6` + whitelist |
| UI env | `management-ui.nix` when accessControl.enable | off/memory |
| CAP_NET_ADMIN on UI | not granted | helper unit only when `nftHelper` (UDS) |

Do **not** invent Q-ACL-1..6 answers. Do not claim live onion, host MDWE, or
live nft bans from unit active alone.

Living maps: [EDGE_AND_TLS.md](../EDGE_AND_TLS.md), [SECURITY.md](../SECURITY.md),
[OPS.md](../OPS.md), `modules/hardening.nix`.

---

## Operator direction (plain)

1. **IP rate limiting** at the edge (middleware: tower-governor or equivalent
   on Axum).
2. **fail2ban-class behavior, stricter:** any **unauthorized access of any
   kind** -> **immediate blacklist** on the box (not gentle multi-retry only).
3. **Explicit whitelist** IPs: never blacklisted; may bypass blacklist (and
   optionally softer rate limits for admin nets).
4. Track **last-used** time on whitelist entries for hygiene and logs so the
   list stays current.
5. Prefer **nftables** integration or crowdsec-like automation; evaluate
   fail2ban vs nft-ban vs **custom Rust** against Surmount principles (own
   stack, Rust preferred long-term).

Scaffold today: `modules/hardening.nix` enables fail2ban with a light **sshd**
jail (`maxretry = 5`, `bantime = 1h`). That is **not** yet merciless product
policy; it is a reversible sketch.

---

## Threat and policy sketch

| Event class | Example | Desired response |
|-------------|---------|------------------|
| Auth failure | Bad SSH key, failed Nostr verify, 401/403 on admin API | Count toward ban; **merciless** lean: fast or immediate ban |
| Probe / unknown route abuse | Scanner hitting `/.env`, WP paths, stale admin paths | Ban after low threshold or signature match |
| Rate flood | Many req/s from one IP on :443 | Rate limit first; escalate to ban |
| Mail auth abuse | SMTP AUTH failures | Prefer Stalwart limits + careful jail; false positives hurt mail |
| Whitelisted IP | Operator home / office / jump host | Never ban; log last-used |

**Unauthorized** needs a precise definition in code later (**Q-ACL-1**). Lean:

- Failed authentication to SSH, product admin API, or other intentional auth
  surfaces.
- Not every 404 (that bans the whole Internet's broken bookmarks). Prefer
  **auth failures + known-hostile patterns + rate escalations**.

Mail plane is special: greylisting and spam are Stalwart's job; global IP ban
on every greylist event would be reckless.

---

## Layered design (defense in depth)

```text
  Internet
     |
     v
  [1] nftables (or iptables-nft)
        - input policy
        - set: surmount-blacklist (timeout optional)
        - set: surmount-whitelist (counters + last-seen if available)
        - whitelist verdict accept early
        - blacklist verdict drop/reject
     |
     v
  [2] Axum edge (tower-governor / custom)
        - per-IP request rate
        - auth failure hooks -> ban publisher
        - security headers, body limits
     |
     v
  [3] App auth (Nostr NIP-98 / session)
        - explicit 401/403 signals for ban logic
     |
     v
  [4] Stalwart anti-abuse (mail only)
        - greylist, spam-filter, protocol limits
     |
     v
  [5] Ban orchestrator (see options below)
        - consumes journald / edge events
        - updates nft set + audit log
        - respects whitelist + last-used
```

SSH stays on its port; edge does not proxy mail ports.

---

## Option comparison: who owns the ban hammer?

| Option | What it is | Pros | Cons | Fit vs principles |
|--------|------------|------|------|-------------------|
| **A. fail2ban** | Python; filters journald/files; actions ban via firewall | In nixpkgs (`fail2ban-1.1.0` on unstable check); NixOS module exists; scaffold already on | Python TCB; jail false positives; "immediate ban any unauthorized" needs aggressive custom jails; not Rust-native | **Transitional OK**; not long-term identity |
| **B. crowdsec** | Collaborative detection + bouncers | Rich scenarios; nixpkgs has `crowdsec` (~1.7.x measured) | Extra daemon; cloud collab story may not match single-VPS sovereignty defaults; not our code | Optional later if measured need |
| **C. nft set + small Rust agent** | Edge/app emits ban events; agent updates `nft` sets; whitelist file with last-used | Own stack; matches Axum-first; merciless policy in one place; last-used natural | We own bugs and signal quality | **Long-term preferred lean** |
| **D. pure tower rate limit only** | No host firewall bans | Simple | No SSH coverage; no drop before accept(); weaker vs scanners | Insufficient alone |

**Recommended path (research lean, not locked):**

1. **Now:** keep fail2ban sshd jail as scaffold; do not expand wild mail jails.
2. **Next (with Axum edge):** implement **rate limits** in tower (governor).
3. **Target:** **Surmount ban helper in Rust** (could live in `surmount-edge`
   or a tiny `surmount-guard` crate) that:
   - Maintains whitelist (CIDR + comment + `last_used` timestamp)
   - On unauthorized signal: if not whitelisted -> add IP to nft set immediately
   - Logs structured events to journald
   - Exposes admin API or CLI for whitelist hygiene (list stale last-used)
4. fail2ban can remain for SSH until the Rust path covers SSH auth logs too,
   then delete Python from this role if clean.

---

## Whitelist model

```text
# proposed data shape (file or small sqlite; not locked)
# cidr            label              last_used              notes
# 203.0.113.10/32  home-office       2026-07-30T12:00:00Z   primary
# 198.51.100.0/24  lab               2026-07-01T09:00:00Z   stale?
```

| Rule | Detail |
|------|--------|
| Never blacklist | If source IP matches whitelist CIDR, ban actions no-op |
| Bypass | Optional: skip rate limit or use higher ceiling for whitelist |
| last-used | Update on any accepted connection from that CIDR (edge + SSH) |
| Hygiene | Ops job / admin UI: show entries with last-used older than N days |
| Bootstrap | First operator IPs from deploy secrets or host config; not public |

Open: store whitelist in Nix config only (rebuild to change) vs mutable
runtime file (**Q-ACL-2**). Lean: **Nix for initial**, runtime file or
Surmount state for last-used updates without rebuild.

---

## Merciless ban parameters (starting sketch)

| Knob | Starting lean | Notes |
|------|---------------|-------|
| Auth failure threshold | **1** for admin HTTPS; **1-3** for SSH | "Immediate" for product admin; SSH might allow 1 typo |
| Ban duration | **24h** or permanent until manual clear | Permanent needs good whitelist discipline |
| Ban scope | Input drop on all public ports | Or drop new connections only |
| IPv6 | Ban /128; consider /64 policy carefully | Do not ban entire ISPs by accident |
| NAT / CGNAT | Shared IPs: merciless hurts neighbors | Whitelist and observation matter |
| False positive recovery | Operator unlist via console / unlocked SSH from whitelist | Document in OPS |

---

## nftables sketch (illustrative, not production module)

```nft
table inet surmount_guard {
  set whitelist {
    type ipv4_addr
    flags interval
    comment "never ban; updated by surmount-guard"
  }
  set blacklist {
    type ipv4_addr
    flags timeout
    timeout 24h
  }
  chain input {
    type filter hook input priority -10; policy accept;
    ip saddr @whitelist accept
    ip saddr @blacklist drop
  }
}
```

IPv6 twin sets required. Wire `surmount-guard` with `nft -j` or `netlink`.
Prefer **nftables** over legacy iptables-only; NixOS firewall can backend
nft.

Do **not** ship this verbatim without integration tests against
`networking.firewall`.

---

## Axum edge hooks

| Middleware | Job |
|------------|-----|
| tower-governor (or equivalent) | Per-IP rate limit on :443/:80 |
| Auth extractor failures | Emit `BanSignal { ip, reason, surface }` |
| Redirect :80 -> :443 | Graceful; do not ban bare HTTP redirects |
| Health / ACME paths | Allow; do not ban LE/HTTP-01 validators (whitelist or path exception) |

ACME HTTP-01 comes from CA infrastructure IPs that change; path-based allow
for `/.well-known/acme-challenge/` is safer than banning unknown GETs there.

---

## fail2ban today vs target

| | Scaffold (`hardening.nix`) | Merciless target |
|--|---------------------------|------------------|
| SSH | maxretry 5, bantime 1h | Lower retries; align with whitelist |
| HTTPS | none | Edge + ban orchestrator |
| Mail | none (Stalwart first) | Careful; high false-positive risk |
| Whitelist | fail2ban ignoreip manual | First-class with last-used |
| Language | Python | Prefer Rust long-term |

Header comments in `hardening.nix` should point at this doc so agents do not
treat the light jail as final policy.

---

## What not to do

- Ban on every 404 or every spam score tick
- Rely on Cloudflare WAF as required path (directed: no)
- Dual-run crowdsec + fail2ban + custom without a clear owner
- Put whitelist-only management solely on a host you can lock yourself out of
  without provider console
- Claim fail2ban is "Rust native" or that Python is free forever

---

## Open questions

**Q-ACL-1.** Exact definition of "unauthorized access" for immediate ban
(auth fail only vs probes vs both)?

**Q-ACL-2.** Whitelist storage: Nix-only vs mutable runtime + last-used DB?

**Q-ACL-3.** Ban default duration: 24h timeout vs until manual clear?

**Q-ACL-4.** Implement Rust `surmount-guard` in-tree for v1 edge cutover, or
keep fail2ban until after mail is live?

**Q-ACL-5.** IPv6 ban granularity (/128 vs /64) for merciless mode?

**Q-ACL-6.** Should mail AUTH failures ever hit the global blacklist, or stay
Stalwart-local forever?

---

## Related

- `modules/hardening.nix` - current fail2ban sketch
- [EDGE_AND_TLS.md](../EDGE_AND_TLS.md) - rate limit layers
- [OPS.md](../OPS.md) - journald / fail2ban ops
- [SECURITY.md](../SECURITY.md)
- nixpkgs: `fail2ban`, `crowdsec` present; policy still ours
