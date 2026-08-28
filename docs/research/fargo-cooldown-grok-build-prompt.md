# Grok Build prompt: first-class cooldown in fargo

Paste the block under **Prompt** into a Grok Build session whose working
tree is the **fargo** clone (fork of https://github.com/rust-lang/cargo ).
This file is the durable copy. Accessed menhera write-up:
https://www.menhera.org/crates-io-cooldown-proxy-mitigating-supply-chain-attacks/
(accessed: 2026-08-27).

Surmount already uses laptop `replace-with = "menhera-cooldown"` and a git
`[patch]` for `chacha20` 0.10.2. Fargo must make that class of policy a
built-in, not a proxy URL plus a whole-crate leak.

## Prompt

```
You are implementing in this tree: fargo, a Surmount fork of rust-lang/cargo.

Product outcome
---------------
Make registry cooldown a first-class fargo feature so we can avoid the
usual crates.io supply-chain pattern (a brand-new version of a popular
crate that is malware, a hijack, or a bad publish) without losing the
ability to take one specific crate@version when we have reviewed it.

Stock Cargo `replace-with` is whole-source. That is the bug we are
fixing. Do not tell the operator to drop the cooldown, to point
crates-io at a URL, or to `[patch]` git as the only escape. Git patch
stays supported. It is not the product.

What menhera taught us (copy the idea, do not hard-code the vendor)
-------------------------------------------------------------------
Menhera.org runs a public Cargo sparse-index proxy that hides crate
versions younger than N days (1d through 30d, e.g.
sparse+https://index.crates.menhera.org/3d/). Config today is:

  [source.crates-io]
  replace-with = "menhera-cooldown"
  [source.menhera-cooldown]
  registry = "sparse+https://index.crates.menhera.org/3d/"

That works until a legitimate fix is newer than the window. On
2026-08-27 RustCrypto yanked chacha20 0.10.0 and 0.10.1 (SSE2 backend
called an SSE4.1 intrinsic; UB). 0.10.2 is the fix, published the same
day. The cooldown index still had 0.10.x yanked and did not list
0.10.2 yet. Stock Cargo cannot say "everything through the cooldown,
except chacha20 0.10.2 from the original registry, checksum C."

Fargo must support that sentence as a first-class, loud, specific
exception. A whole package name on an allow list is too wide: it would
also take a malicious chacha20 0.10.3 published twenty minutes later.

Do not vendor or require index.crates.menhera.org. Menhera stays a
valid replacement source. Cooldown policy lives in fargo even when
the index is plain crates.io.

Policy (implement this, not a close variant)
--------------------------------------------
1. Default for stock-compatible behavior: cooldown off. Surmount will
   turn it on in config. Do not break `cargo test` in this fork's
   own testsuite by forcing a 3-day window on every resolve.

2. When on, fargo withholds registry versions whose publish time is
   newer than `min-age`. Yanked versions stay yanked. Cooldown is not
   yank. An exception may lift cooldown. An exception must never
   un-yank unless the operator sets a separate, explicit
   `allow-yanked` on that same crate@version (default off).

3. Exceptions are crate + version + checksum + because-text. Not a
   bare crate name. Not a silent fallthrough on "missing from the
   replacement."

4. Fail closed and name the next command. Never fetch a too-new
   crate because resolve got stuck.

Config shape (use these keys; kebab-case in TOML)
-------------------------------------------------
# .cargo/config.toml  (and/or Cargo.toml [workspace.metadata] later;
# config.toml is the first cut, same layer as replace-with)

[cooldown]
enable = true
min-age = "3 days"          # humantime-style; also accept "3d"
# Index used to read publish times and to fetch allowed originals.
# Default: the unreplaced crates-io sparse index.
original = "crates-io"

# Optional: still allow replace-with for the default graph.
# Cooldown applies to versions considered from that graph.

[[cooldown.allow]]
name = "chacha20"
version = "0.10.2"
checksum = "sha256:<64 hex>"   # crates.io package checksum, same as Cargo.lock
because = "RustCrypto 0.10.2 fixes yanked 0.10.0/0.10.1 SSE2 UB; window not elapsed"
# optional: until = "2026-09-03"  # after this, the exception expires and the version must pass min-age or resolve fails

CLI (first-class, not hidden config only)
----------------------------------------
Keep the `cargo` binary name so rustup and existing scripts work.
`fargo` may be a second binary or a documented alias; do not rename
every test to fargo in the first cut.

  cargo cooldown
      Print whether cooldown is on, min-age, original index, and the
      allow list (name, version, checksum prefix, because).

  cargo cooldown why <name>@<version>
      One of: allowed (checksum match), too-new (age vs min-age),
      yanked, not-on-index, on-replacement. Never a vague miss.

  cargo cooldown allow <name>@<version> --checksum sha256:<hex> --because "..."
      Writes the exception into the nearest .cargo/config.toml
      (or --config path). Refuses without checksum and because.
      Refuses if that version is yanked unless --allow-yanked is
      also passed.

  cargo update / cargo generate-lockfile
      If the resolver wants a version that is too new and not
      allowed, fail with:

        error: chacha20 0.10.2 was published 4 hours ago (cooldown min-age is 3 days)
        this is the usual crates.io supply-chain window
        to take this exact version after you have reviewed it:
          cargo cooldown allow chacha20@0.10.2 --checksum sha256:... --because "..."
        to stay on the previous graph, do not pass --aggressive flags

Resolve and lockfile
--------------------
- replace-with still remaps the default graph. Do not remove it.
- For an allowed crate@version, query/download from the original
  crates-io sparse index (or `cooldown.original`), verify checksum,
  then record in Cargo.lock as a normal crates-io package id plus
  checksum (same as today). Machines without cooldown still build.
- Machines with cooldown enable=true MUST have the same allow entry
  (name, version, checksum) or resolve must fail: "chacha20 0.10.2
  is newer than min-age and is not in cooldown.allow".
- cargo metadata / cargo tree: show that the node used original
  because of cooldown.allow (verbose or a note is enough; do not
  lie that it came from the replacement if it did not).
- Do not encode a silent crates-io leak. Do not make
  --registry crates-io the resolve escape (that flag is publish/API).

Where the code lives in this cargo tree
---------------------------------------
  src/cargo/sources/config.rs     parse [source.*] and new [cooldown]
  src/cargo/sources/replaced.rs   ReplacedSource query/download
  src/cargo/core/source_id.rs     crates-io vs replacement
  src/cargo/core/resolver/        where version picks happen
  src/cargo/ops/cargo_generate_lockfile.rs / update path
  src/doc/src/reference/source-replacement.md
  new: src/doc/src/reference/registry-cooldown.md
  new: src/bin/cargo/commands/cooldown.rs (or equivalent clap command)
  crates/cargo/tests/testsuite/   existing source-replacement tests

Publish time: prefer the sparse-index pubtime / created field Cargo
already understands. If a version has no publish time, fail closed
when cooldown is on (do not treat missing time as old enough).
Do not call random crates.io web HTML. Index JSON or the registry
API Cargo already uses only.

Tests (red first, then product, then green). Land these.
--------------------------------------------------------
In crates/cargo/tests/testsuite/, hermetic fake indexes (two
registries: original with a fresh 0.10.2, replacement/cooldown
index that hides it or still has yanked 0.10.0/0.10.1 only):

  cooldown_hides_fresh_version
      enable + min-age 3 days. Fresh crate is not selected.
      Error names cargo cooldown allow.

  cooldown_allow_exact_checksum
      same indexes. [[cooldown.allow]] for name+version+checksum.
      Resolve takes 0.10.2 from original. Checksum matches.
      A different checksum on the allow line fails.

  cooldown_allow_does_not_unyank
      0.10.1 yanked on both. allow for 0.10.1 without
      allow-yanked still fails. With allow-yanked, succeeds
      (document this as dangerous).

  cooldown_allow_does_not_open_other_versions
      allow 0.10.2 only. Fresh 0.10.3 on original is still
      withheld.

  cooldown_off_is_stock_cargo
      enable = false. replace-with still works as upstream.
      No new errors.

  cooldown_why_cli
      cargo cooldown why foo@1.2.3 prints the distinct
      too-new / allowed / yanked cases.

Do not rewrite tests to match a weaker design. Do not add
allow-original = ["chacha20"] as the shipped product (too wide).
A private helper that keys on package name internally is fine
only if the public config still requires version+checksum.

What not to do
--------------
- Do not remove replace-with.
- Do not require the menhera hostname.
- Do not silent-fallthrough when the replacement 404s or yanks.
- Do not use [patch] as the implementation of cooldown.allow.
- Do not run git add or git commit. Leave the tree for a human.
- ASCII in docs we add. No em dashes. Plain American English.
- Do not claim crates.io is unsafe without the test evidence
  (too-new withheld, exception checksum-enforced).

Order of work
-------------
1. Failing tests for the six names above (they will not compile
   or will fail until commands/config exist; that is the red).
2. Parse [cooldown] and [[cooldown.allow]].
3. Wire resolve: withhold too-new; honor exact allow.
4. cargo cooldown / cargo cooldown why / cargo cooldown allow.
5. Book page registry-cooldown.md linked from source-replacement.md.
6. Re-run the same tests. Report: test names, red evidence, green
   evidence. If something is blocked on missing pubtime in the
   fake index fixture, say that and add pubtime to the fixture.

When you are done, print:
- files touched
- the exact config a Surmount laptop would use for 3-day cooldown
  plus chacha20 0.10.2 (checksum placeholder if the test hash is
  synthetic)
- leftover: anything not in this prompt (rustup dist, nix pin of
  the fargo binary) as operator work, not as silent scope.
```
