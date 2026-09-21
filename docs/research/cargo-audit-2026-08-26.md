# cargo-audit (RustSec) 2026-08-26

Hermetic run: `cargo-audit audit --no-fetch --stale` against flake input
`advisory-db` (github:rustsec/advisory-db) and root `Cargo.lock`.
Loaded 1226 advisories. Operator command: `just audit` (or
`just audit-remote`).

`just audit` builds `checks.*.cargo-audit` and **is** in `checks.*.ci`
after nostr 0.44.8, time 0.3.55, and `h2` 0.4.16 (RUSTSEC-2026-0258
cleared on cargo update 2026-08-27). `just deny` stays in `checks.ci`.

## 2026-09-21 rustls RUSTSEC-2026-0285

[RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285)
(accessed: 2026-09-21): rustls 0.23.13 through 0.23.44 accepted TLS 1.3
handshake messages across encryption-level boundaries. Patched
`>=0.23.45`. crates.io and menhera **3d** list 0.23.45 (published
2026-09-14T15:11:17Z, checksum
`0d41d731c7d2f962d1ccc364cec258de3c0e93b38c2fb3ba97ac74513048d634`).
Laptop `replace-with` is menhera **10d**; named registry is **7d**. Both
still stop at 0.23.44.

`cargo update -p rustls --precise 0.23.45` against that replacement:

```
error: no matching package named `rustls` found
location searched: `menhera-cooldown` index (which is replacing registry `crates-io`)
```

Do **not** ignore the advisory. Do **not** fetch crates.io to skip
menhera. Git `[patch.crates-io]` of
https://github.com/rustls/rustls rev
`2976d90fd1c2db6b518700dd101b714069cfcb17` (tag `v/0.23.45`) is the
same escape as chacha20. Workspace pin is `0.23.45`. When menhera 10d
lists 0.23.45, drop the rustls patch line and
`cargo update -p rustls --precise 0.23.45`.

## 2026-08-27 unmaintained warnings

Operator `cargo audit` paste and hermetic `just audit` (same three IDs,
exit 0, "3 allowed warnings"):

| ID | Crate | Parent | This turn |
|----|-------|--------|-----------|
| [RUSTSEC-2024-0384](https://rustsec.org/advisories/RUSTSEC-2024-0384) | instant 0.1.13 | nostr 0.44.8 -> management-ui | **not dropped.** nostr 0.45.3 uses `universal-time` instead of instant, but removes TagKind / TagStandard / JsonUtil / `EventBuilder::sign_with_keys` and gates `Keys::generate` behind `os-rng` (rand 0.10). That is a NIP-98 rewrite, not a lockfile bump. Latest 0.44 is 0.44.8. |
| [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436) | paste 1.0.15 | leptos 0.8.20 / tachys / reactive_* | **not dropped.** 0.8.20 is latest 0.8. 0.9.0-beta is not a product bump. `[patch]` of paste to pastey would rename a crate leptos still imports as `paste`. Not honest. |
| [RUSTSEC-2026-0173](https://rustsec.org/advisories/RUSTSEC-2026-0173) | proc-macro-error2 2.0.1 | leptos_macro / rstml / syn_derive **and** (before) i18n-embed-fl / age | **age path dropped.** Workspace `age` 0.12.1 pulls i18n-embed-fl 0.10.1 (`proc-macro-error3`). pme2 remains via leptos 0.8.20. |

**No cargo-audit ignore.** Unmaintained is a real warning. An ignore would
silence `just audit`. Leave the IDs printing until the parent crate
drops them.

`age` 0.12.1 also pulls `kem 0.3.0-pre.0` via `ml-kem` 0.2.3 (upstream
published graph; rand stays 0.8). chacha20 git patch unchanged.
Accessed: 2026-08-27.

## Vulnerabilities (cargo-audit error)

| ID | Crate | Locked | Notes |
|----|-------|--------|-------|
| RUSTSEC-2026-0258 | h2 0.4.15 | hyper | empty DATA frames; upgrade >=0.4.16 |
| RUSTSEC-2026-0227 | nostr 0.43.1 | management-ui | NIP-44 v2 resource exhaustion; >=0.44.7 |
| RUSTSEC-2026-0225 | nostr 0.43.1 | | Debug may expose NIP-46/60 creds; >=0.44.7 |
| RUSTSEC-2026-0228 | nostr 0.43.1 | | NIP-04 parse memory; >=0.44.7 |
| RUSTSEC-2026-0216 | nostr 0.43.1 | | NIP-44 v2 DoS; >=0.44.5 |
| RUSTSEC-2026-0226 | nostr 0.43.1 | | wallet event parsers; >=0.44.7 |
| RUSTSEC-2026-0229 | nostr 0.43.1 | | NIP-98 parse exhaustion; >=0.44.7 |
| RUSTSEC-2026-0230 | nostr 0.43.1 | | empty NIP-50 search panic; >=0.44.7 |
| RUSTSEC-2026-0219 | nostr 0.43.1 | | NIP-04 IV DoS; >=0.44.6 |
| RUSTSEC-2026-0009 | time 0.3.45 | rcgen/x509-parser/UI | stack DoS; >=0.3.47 |

Workspace today **pins** `nostr = "0.44.7"` (lock 0.44.8) and `time`
`0.3.47+`. Audit **is** a CI gate. The 0.43 / exact-time-0.3.45 notes
below are historical from 2026-08-26.

## Warnings (did not fail `--deny` vulns; unmaintained/unsound)

| ID | Crate | Kind |
|----|-------|------|
| RUSTSEC-2024-0384 | instant 0.1.13 | unmaintained (via nostr 0.44.8; still in lock) |
| RUSTSEC-2024-0436 | paste 1.0.15 | unmaintained (via leptos 0.8.20; still in lock) |
| RUSTSEC-2026-0173 | proc-macro-error2 2.0.1 | unmaintained (via leptos_macro / rstml; age path dropped) |
| RUSTSEC-2026-0221 | event-listener 5.4.1 | unsound `!Send` (via leptos reactive_graph). **Not printed** by `just audit` 2026-08-27 (1226 advisories). |

## Operator leftovers

1. Rewrite management-ui NIP-98 helpers onto nostr 0.45.3 (or later 0.45)
   so `instant` leaves the lock. Do not enable `os-rng` without checking
   rand 0.10 / chacha20 0.10 against menhera-cooldown.
2. When leptos publishes a **stable** 0.9 that drops `paste` and
   `proc-macro-error2`, bump workspace `leptos` off 0.8. Do not ship
   0.9.0-beta as that bump.
3. `h2` >=0.4.16, nostr 0.44.7+, `time` 0.3.47+, and `checks.*.ci`
   already landed. Do not re-do those.

URLs: https://rustsec.org/advisories/<ID>
Accessed: 2026-08-26; unmaintained re-check 2026-08-27.
