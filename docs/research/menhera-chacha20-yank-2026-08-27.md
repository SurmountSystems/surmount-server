# cargo update vs menhera-cooldown (2026-08-27)

Operator `cargo update` from the repo root (workspace lock is `Cargo.lock`).
Private index `menhera-cooldown` replaces crates-io (laptop
`~/.cargo/config.toml`). Do **not** undo that replacement from this tree.

## Why 0.10.0 / 0.10.1 were yanked (public, not menhera-only)

This is **not** a mystery CVE on a private index. **RustCrypto yanked
those versions on crates.io the same day** (2026-08-27).

Public sources:

- CHANGELOG: `chacha20` **0.10.0** and **0.10.1** marked **YANKED**.
  **0.10.2** (2026-08-27) fixes "Use of SSE4.1 intrinsic in SSE2 backend
  of RNG and legacy (64-bit counter) variants"
  (https://github.com/RustCrypto/stream-ciphers/blob/master/chacha20/CHANGELOG.md).
- PR [#580](https://github.com/RustCrypto/stream-ciphers/pull/580) /
  issue [#579](https://github.com/RustCrypto/stream-ciphers/issues/579):
  SSE2 backend used an SSE4.1 intrinsic (undefined behavior on SSE2-only
  CPUs). Hits **RNG** and **legacy 64-bit counter** variants. Author
  noted tests did not catch writing only 32 counter bits for those
  variants.
- Yank commit [#583](https://github.com/RustCrypto/stream-ciphers/pull/583):
  "The yanked versions contain UB in SSE2 backend, see #580."
  newpavlov asked to yank 0.10.0/0.10.1 as UB; tarcieri agreed.

**No RUSTSEC / CVE id found yet** for this 2026-08-27 yank (advisory-db
may lag). The only RustSec on crate `chacha20` is still
**RUSTSEC-2019-0029** (counter overflow, **patched >= 0.2.3**). That is
**not** this bug. Do not confuse:

- RUSTSEC-2026-0124 / GHSA-hc3c-63hc-2r9f: **libcrux-chacha20poly1305**,
  different crate.
- RUSTSEC-2021-0121: **crypto2**, different crate.
- CVE-2020-12403: **NSS** C library, not this crate.

crates.io security page for `chacha20` still lists only 2019-0029.

**menhera mirroring crates.io yanks is doing its job.** 0.10.2 is the
intended replacement. The index had not listed 0.10.2 yet. This tree
lands 0.10.2 via **git `[patch.crates-io]`** of
`RustCrypto/stream-ciphers` rev `6b236b758a0279f64d777797514813b2cb572c8b`
(not the crates.io tarball). Then `cargo update -p quinn-proto --precise
0.11.17`. When menhera has 0.10.2, drop the patch.

## What the resolver did

| Crate | Before (git HEAD lock) | After `cargo update` |
|-------|------------------------|----------------------|
| quinn-proto | 0.11.16 | 0.11.15 |
| rand | 0.8.7 and 0.10.2 | 0.8.7 and 0.9.5 |
| chacha20 | 0.9.1 and 0.10.1 | 0.9.1 only |
| getrandom | 0.2.x (+ maybe 0.4) | 0.2.17, 0.3.4, 0.4.3 |

`cargo update -p quinn-proto --precise 0.11.17` and `--precise 0.11.16`
**fail** on this index:

```
chacha20 = "^0.10.0"
version 0.10.0 is yanked
version 0.10.1 is yanked
location searched: menhera-cooldown index (which is replacing registry crates-io)
required by rand v0.10.1
required by quinn-proto v0.11.16 / 0.11.17
via quinn 0.11.11 <- reqwest 0.12.28 <- surmount-management-ui
```

`cargo metadata --offline` on the post-update lock **succeeds**.

## Assumptions (tested)

1. **crates.io still lists chacha20 0.10.1 as current** (crates.io crate page,
   not yanked there). Test: public registry pages, not a fetch into this
   build.
2. **This laptop never talks to crates-io for resolve** (`source.crates-io
   replace-with = menhera-cooldown`). Test: cargo error names menhera, not
   crates-io.
3. **Yank on menhera is policy, not a Cargo bug.** Not proven *why* 0.10.x
   was yanked. Do **not** `[patch]` crates-io or drop `replace-with` to
   "get 0.11.17 back." That would bypass the supply-chain index.
4. **0.11.15 is the newest quinn-proto that does not need yanked chacha20
   0.10.** Test: precise 0.11.16/0.11.17 refuse; lock settled on 0.11.15
   with rand 0.9.5.

## What not to do from this repo

- Do not add a crates-io source or git patch for chacha20 0.10.1.
- Do not pin `quinn-proto = "0.11.17"` in Cargo.toml (it will not resolve).
- Do not `cargo update --offline` to keep yanked 0.10.1 unless the operator
  explicitly wants yanked crates in the lock.

## Operator (index), if 0.11.16+ is required

On **menhera-cooldown**: un-yank or re-publish **chacha20 0.10.1** after
reviewing why it was yanked. Then `cargo update -p quinn-proto --precise
0.11.17`. Until then, 0.11.15 is the honest graph.
