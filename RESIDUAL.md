# Residual (open work after Phases A-D foundation + review fix rounds)

**Last updated:** 2026-08-07 (account mutation e2e hermetic anchors shipped;
mutations + structured request logging already in tree; prior: JMAP 501
honesty; live Stalwart directory + wire-mock; production :80 free pin; e2e
Nostr + onion anchors; e2e SoT Rust `crates/surmount-e2e`; no staging/VPS)
**Status:** living residual after nginx default-off cutover wiring + prior
foundation. Not operator acceptance of unfinished host items. **No cutover
claimed.**

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
  **Not JS NDK.** UI honesty: auth-mode banners, `/login` when nostr, no
  create-mailbox forms. Q-AUTH-1 residual: key-loss, durable session store,
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
  (nostr or lab escape) + CSRF on cookie POSTs; no HTML form; no password/nsec.
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
  is Surmount-owned Arti **2.5.0** source build + cargo feature
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
- Residual: **live Tor network verification** (unit active != published);
  operator host HS keys + ownership; systemd hardening parity after real
  `arti proxy`; Q-ARTI-*. Local e2e may publish with temp keys when arti is
  present; that is not host `surmount-arti` ownership.

**Tooling**

- `justfile` + flake `checks.ci` aggregate
- crane cargo test/clippy/fmt checks

---

## What remains

| Track | Residual |
|-------|----------|
| **Public HTTPS host cutover** | Tree default is nginx off + https UI path; **operator host** still must place certificate/key files (PEMs), bind public :443, confirm MemoryDenyWriteExecute hardening (no writable+executable memory) via `just e2e-host`, choose cert path (Q-EDGE-1 / Q-CA-*). Do not claim live public cutover from eval or local e2e alone |
| **nginx module delete from tree** | Dual-run escape still ships (`web.enable = true`). Unused-detection checklist: [docs/OPS.md](docs/OPS.md). Delete module file only after operators no longer need it **and** explicit operator OK |
| **:80 redirect listener** | **Wired** in tree (flag + listen + allowlist; dual-run mutex; redirect-only). **Operator assumption (2026-08-02):** production port 80 is **free** for that bind. Host public proof still residual. **ACME HTTP-01 on product :80 parked** (Q-EDGE; free :80 does not invent ACME) |
| **Arti live HS** | Live Tor verify on operator host; operator HS keys/ownership for surmount-arti; hardening after real `arti proxy`; admin/JMAP stanzas if ever wanted. **Package currency shipped:** Surmount-owned Arti **2.5.0** source + `onion-service-service` (`artiOnionService`; not nixpkgs 1.4.2 lag). **Tree cleartext local backend for https+Arti auto-path shipped** (loopback API + onion target; host still must place HS keys and prove Tor). Local temp-key publish (optional `just e2e` row) != host ownership. Do not invent Q-ARTI-2/3 answers. **Onion URL surface (read-path) shipped (2026-08-01):** optional `SURMOUNT_ONION_URL` / `SURMOUNT_ONION_HOSTNAME_FILE` + Nix options; console system/overview + `GET /api/v1/system` show operator-published address only when set; redaction helper for logs. Still residual: host publish proof, HS keys, live Tor verify. Do **not** invent a live onion in the tree; do **not** log full onion addresses in failure tails |
| **Merciless ban product** | **First path + helper scaffold shipped:** Rust decide + memory/file; optional kernel firewall sync via Unix-socket helper oneshot or unsupported direct exec; Nix `accessControl` + sets; `nftHelper` requires `backend=nft` (fail-closed); UI no CAP_NET_ADMIN; EEXIST/already-present treated as apply ok for crash-window re-signal; **`remove_ban` / lab unban shipped** (hermetic delete-element + absent=ok). **Auth-failure BanCandidate stub shipped** + surface audit (404/501 do not auto-ban); bad NIP-98 / session exchange may call `signal_unauthorized` once (missing cookie does not). **Still residual:** Q-ACL-1..6 (which surfaces call the hook); live host helper + sets drop (`just e2e-host`); fail2ban SSH transitional. **Not** live host drop / full unauthorized auto-ban product |
| **Admin Leptos SSR** | **Multi-page console + UI depth (2026-08-01):** SSR `/`, `/domains`, `/accounts`, `/system`, `/mail`, `/login` with DOGE chrome, status chips (UI / Stalwart / onion configured marker), inventory cards, definition-list system page, mail probe card. No skeleton branding. Accounts honest empty (`source: unavailable`) by default; domains config inventory only. **Directory trait + hermetic mock + live client shipped (2026-08-07):** `AppState.directory` (`unavailable` default; labeled `mock` fixture; live `stalwart` management JMAP with host token, never default-on; list fail-closed empty). **Account create/update mutations shipped (2026-08-07):** `Directory::create_account` / `update_account`; `POST /api/v1/accounts` + `PATCH /api/v1/accounts/{id}`; mock + live `x:Account/set` wire-mock; fail-closed when `auth_mode=off` unless lab `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED`; double-submit CSRF when session cookie present (same helper as logout). No HTML create form; no password/nsec on wire. Docs scrub: README/STACK describe real console; Stalwart admin = bootstrap fallback. Directory research: [docs/research/stalwart-directory-api.md](docs/research/stalwart-directory-api.md). Day-one host order in [docs/OPS.md](docs/OPS.md). **Still residual:** hydrate islands only if clear SSR gap (parked); full Q-AUTH-1 product answers; JMAP proxy + webmail **beyond** thin 501 (501 honesty locked); **host cutover / hardware deploy still open for operator**; host must place Stalwart API token when enabling live directory |
| **Nostr E2E auth** | **Foundation shipped (2026-08-01):** `SURMOUNT_AUTH_MODE` off (default) / nostr; rust-nostr NIP-98 verify (kind 27235); env allowlist fail-closed when empty; HMAC session cookie scaffold; challenge/session/logout/me + `/login`. **Not JS NDK.** **Security headers + CSRF logout shipped (2026-08-01):** baseline CSP (nonce for login script) / nosniff / Referrer-Policy / frame denial on management router; double-submit CSRF on `POST /api/v1/auth/logout` (session cookie remains HttpOnly + SameSite=Lax; Secure when HTTPS). **CSRF on account mutation POSTs shipped (2026-08-07)** (cookie session requires `X-CSRF-Token` / body `csrf`). **Structured request logging shipped (2026-08-07):** method/path/status/latency; onion-redacted path; no Authorization/Cookie/bodies. **Q-AUTH-1 residual (do not invent):** key-loss recovery; durable server session store choice; first-operator bootstrap product UX. nsec never on server. Full webmail CSP still residual. |
| **JMAP proxy + v1 webmail** | **Thin 501 boundary shipped / locked:** `POST /api/v1/jmap` returns honest 501 (`error: jmap_proxy_not_implemented`), mail SSR documents residual, `/webmail` is 404 (no fake UI), surface audit no auto-ban on 501. Hermetic: `jmap_proxy_returns_honest_501_body` + `mail_page_documents_jmap_proxy_501_residual` + surface audit. **Beyond 501 (real authenticated proxy + v1 webmail UI) parked** offline-capable; do not invent. |
| **Vaultwarden / LUKS live / PQConnect / migration / UDS Stalwart / search / multi-host** | As before |

---

## Highest-value next (with acceptance criteria)

Shipped in tree (do **not** re-list as next work): in-process rustls HTTPS,
Arti HS management-publish + package overlay, **https+Arti auto local
cleartext backend**, nginx default-off / dual-run escape, :80 redirect-only
bind, ban decision first path + helper `add_ban`/`remove_ban`/`ping`, Leptos
SSR **multi-page management console** (not skeleton), surface audit (no
auto-ban on 404/501), **live Stalwart directory client** (explicit opt-in),
**JMAP thin 501 honesty contracts**, **account create/update mutations**
(auth + CSRF), **structured request logging**, hermetic e2e-host pure helpers.
Details under **What shipped**.

**Ranking (2026-08-07) while VPS pending:**

| Bucket | Highest-value next for parallel agents |
|--------|----------------------------------------|
| **Ship (offline)** | **Empty for product code** after mutation e2e anchors. Optional: CSP/hardening depth only if operator asks; do not invent Q-AUTH-1 or principal fields |
| **Park (offline)** | Full JMAP proxy + v1 webmail UI beyond 501; hydrate islands without a clear SSR gap; inventing any Q-* |
| **Host-gated (highest value now)** | Public HTTPS cutover + `just e2e-host`; Arti HS keys + live Tor verify; ban enforce lab drop proof; live directory token + mutation proof on host |
| **Operator handoff** | Stage/commit untracked `auth.rs` + `directory.rs` so flake `just check-ci` is green (agents never stage). Host `just check` still sees the working tree |

**Shipped offline this slice (2026-08-07):** account create/update mutations
(directory trait + mock + Stalwart `x:Account/set` wire-mock; auth gate + lab
escape; CSRF on cookie POSTs); structured `http_request` logging with onion
redaction (no secret paths/tokens/nsec). **Mutation e2e hermetic anchors** in
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

- [ ] Public :443 happy path on operator VPS without `surmount.web.enable`
- [ ] Free production :80 binds product **redirect-only** listener when enabled
      (operator free-:80 assumption; not ACME-on-product-:80)
- [ ] Certificate and key files on host only; key mode not group/world
      readable; never in git
- [ ] `systemctl show surmount-management-ui -p MemoryDenyWriteExecute` => yes
- [ ] TLS health curl to loopback and/or public name
- [ ] nginx not product edge when web off
- [ ] Cert renew path for chosen CA flow (Q-EDGE-1 / Q-CA-*); ACME-on-Axum-:80
      still parked
- [ ] Optional: remove `modules/web.nix` only after dual-run unused

### 2. Arti live Tor verify (operator)

Module + package overlay shipped. Local optional Tor row (`just e2e`) may
publish with **temp keys** when arti + client exist; that does **not** close
host residual.

Still residual (host):

- [ ] Live Tor network verification (**unit active != published**)
- [ ] Operator-placed HS identity under onionServiceStateDir, owned/writable
      by `surmount-arti`; never in git
- [x] Cleartext local backend when managementUi is https-only (tree:
      auto loopback cleartext API + Arti backend; host must still run the
      composition and prove onion fetch)
- [ ] systemd hardening parity after real `arti proxy`
- [ ] Q-ARTI-2 / Q-ARTI-3 not invented; admin/JMAP onion stanzas still off
- [ ] When HS publishes: surface onion URL to operator (admin UI status
      and/or docs path). Do not invent live onion now; do not log onion
      in failure tails

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
- [ ] Leptos hydrate islands only where a clear SSR gap needs them (none forced
      this pass; DOGE SSR primary; no NPM)
- [x] Nostr auth foundation (mode/allowlist/NIP-98/session cookie; rust-nostr);
      e2e hermetic anchors shipped for off/gate/NIP-98 session + surface audit
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
| **Q-EDGE-1** / **Q-CA-*** | Cert sharing / public CA |
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
| :80 redirect-only wiring + dual-run mutex | **Shipped** (host public bind still residual) |
| Production free-:80 docs pin (redirect-only; not ACME invent) | **Pinned 2026-08-02** |
| CSP / CSRF / session hardening beyond scaffold | Residual product when other agents touch auth/UI; not host-gated |
| JMAP thin 501 honesty + route contracts | **Shipped / locked 2026-08-07** (not full proxy) |
| JMAP authenticated proxy + v1 webmail UI beyond 501 | Residual offline-capable (**parked** depth; do not invent) |
| Hydrate islands | **Parked** until a clear SSR gap; DOGE SSR primary; no NPM |
| Optional local Tor deep row when arti + Tor client on PATH | Local only; != host `surmount-arti` ownership |
| Q-AUTH-1 / Q-ACL / Q-ARTI / Q-EDGE answers | **Do not invent**; park |
| Untracked module sources in dirty tree (`auth.rs`, `directory.rs`) | **Operator stage/commit handoff** so flake `just check-ci` sees them (agents never stage). Host `just check` / `cargo test` use the working tree |

### Host-gated (when the VPS arrives)

Require real deploy + `SURMOUNT_E2E_HOST=1` / operator secrets. Agents without
a host **stop** at docs, tree wiring, and local e2e.

1. **Public HTTPS cutover + `just e2e-host`** (PEMs, MemoryDenyWriteExecute,
   public :443; free :80 redirect-only bind when enabled)
2. **Arti HS keys + live Tor verify** (unit active != published)
3. **Ban enforce lab** (sets + membership + traffic drop proof; cleanup via
   `surmount-nft-ban-helper remove-ban <ip>`)
4. Cert renew path for chosen CA flow (Q-EDGE-1 / Q-CA-*); ACME-on-product-:80
   still parked
5. Delete `web.nix` only when dual-run unused **and** explicit operator OK
   (checklist: [docs/OPS.md](docs/OPS.md))

### Agent-done earlier (packaging + docs; no cutover)

- [x] Version currency audit vs network latest (Stalwart family at tip;
      crane rustc 1.88 vs stable 1.97.1 noted in pre-26.05 audit; living
      crane is 1.95 via `rustPackages_1_95`, see
      [docs/COMPACTION-PIN.md](docs/COMPACTION-PIN.md)). Snapshot:
      [docs/research/version-audit.md](docs/research/version-audit.md)
- [x] **Arti packaging currency:** Surmount-owned **2.5.0** source build
      (`nix/packages/arti-onion-service.nix`, GitLab `arti-v2.5.0`) +
      `onion-service-service`; rustc via `nixpkgs-rust` (MSRV 1.91+); not
      full OS channel bump. Eval contract locks version 2.5.0. Live Tor
      verify still residual
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
