# cargo-audit (RustSec) 2026-08-26

Hermetic run: `cargo-audit audit --no-fetch --stale --deny warnings`
against flake input `advisory-db` (github:rustsec/advisory-db) and root
`Cargo.lock`. A warning fails that check. Loaded 1226 advisories on the
2026-08-27 run. Operator command: `just audit` (or `just audit-remote`).

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

That paragraph describes the 2026-08-27 lock. The current lock is the
2026-09-26 section.

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

Workspace today pins nostr `0.45.3` (lock 0.45.5) and `time`
`0.3.47+`. Audit **is** a CI gate. The 0.43 / exact-time-0.3.45 notes
below are historical from 2026-08-26.

## Warnings

`checks.*.cargo-audit` passes `--deny warnings`. A warning fails the
derivation. As of 2026-09-26, instant is not a current lock hit. paste
and proc-macro-error2 are in the lock and ignored. See the 2026-09-26
section. Do not ignore any other advisory.

| ID | Crate | Kind |
|----|-------|------|
| RUSTSEC-2024-0384 | instant 0.1.13 | unmaintained. Not in the lock (nostr 0.45.5, universal-time). |
| RUSTSEC-2024-0436 | paste 1.0.15 | unmaintained INFO. In the lock via Leptos. Ignored 2026-09-26. Not a CVE. |
| RUSTSEC-2026-0173 | proc-macro-error2 2.0.1 | unmaintained INFO. In the lock via Leptos. Ignored 2026-09-26. Not age. Not a CVE. |
| RUSTSEC-2026-0221 | event-listener 5.4.1 | unsound `!Send` (via leptos reactive_graph). **Not printed** by `just audit` 2026-08-27 (1226 advisories). |

## 2026-09-26 vendor reverted, two ids ignored

On 2026-09-26 the third_party Leptos fork was reverted. either_of,
leptos, leptos_macro, reactive_graph, reactive_stores,
reactive_stores_macro, syn_derive, and tachys come from crates.io
again, at the same versions as before the fork (leptos 0.8.20 and the
matching published crates). paste 1.0.15 and proc-macro-error2 2.0.1
are back in the lock the way upstream publishes them. pastey is not in
the lock. proc-macro-error3 3.1.1 stays because i18n-embed-fl 0.10.1
uses it. The rustls git patch and the chacha20 git patch stay.

The fork existed only so cargo-audit would stop naming
RUSTSEC-2024-0436 and RUSTSEC-2026-0173. That was the wrong size of
change. Do not vendor Leptos to clear these two ids.

Both records are RustSec type INFO, informational "unmaintained". They
are not CVEs. The OSV records have cvss null and no CVE alias.

paste (RUSTSEC-2024-0436) pastes identifiers inside a compile-time
macro. The dtolnay/paste repo was archived on 2024-10-06. The advisory
text is only that the project is unmaintained. It does not describe a
vulnerability.
https://rustsec.org/advisories/RUSTSEC-2024-0436

proc-macro-error2 (RUSTSEC-2026-0173) prints compiler errors when a
procedural macro fails. The author confirmed it is unmaintained. The
advisory does not describe an exploitable bug. This id is
proc-macro-error2, not age. The age 0.12 bump is a separate change: it
pulls i18n-embed-fl 0.10.1, which uses proc-macro-error3.
https://rustsec.org/advisories/RUSTSEC-2026-0173

Both entered this lock only through Leptos. The management UI is the
only product crate that depends on Leptos. Its source does not call
paste or proc-macro-error. The crates run inside rustc while the UI is
compiled. They are not in the mail server process and not in the running
management UI process. A lockfile pin means a later crates.io upload
does not enter the build until someone updates the lock.

`checks.*.cargo-audit` runs
`cargo-audit audit --no-fetch --stale --deny warnings` with
`--ignore RUSTSEC-2024-0436` and `--ignore RUSTSEC-2026-0173`.
`--deny warnings` stays, so any other warning still fails. The ignore
list is only those two ids. Do not ignore any other advisory. Do not
turn off the whole unmaintained class. `.cargo/audit.toml` records the
same two ids. The hermetic check passes the flags on the command
because that derivation does not see the repo. `deny.toml` stays
bans-only.

Accessed: 2026-09-26.

## Operator leftovers

1. `instant` is gone from the lock (nostr 0.45.5, universal-time). Do not
   re-add it.
2. The Leptos `third_party/` fork was reverted on 2026-09-26. Do not
   vendor Leptos again to clear RUSTSEC-2024-0436 or RUSTSEC-2026-0173.
   Those two ids are ignored. Do not ignore any other advisory. Do not
   ship 0.9.0-beta just to chase paste.
3. `h2` >=0.4.16, nostr 0.45, `time` 0.3.47+, and `checks.*.ci`
   already landed. Do not re-do those.

URLs: https://rustsec.org/advisories/<ID>
Accessed: 2026-08-26; unmaintained re-check 2026-08-27; vendor reverted
and two INFO ids ignored 2026-09-26.
