# Cargo source replacement is whole-source (fork notes)

Cargo is not missing a one-line bug. `replace-with` is **defined** as
replacing an entire **source**, with the lockfile still naming crates-io.

Upstream: https://github.com/rust-lang/cargo
Book: https://doc.rust-lang.org/cargo/reference/source-replacement.html

Quote from that page: "Cargo has a core assumption about source
replacement that the source code is exactly the same from both sources."

That assumption is why you cannot say "all crates-io crates come from
menhera, except chacha20 0.10.2 from crates.io." Git `[patch]` is the
supported escape (different source kind). Per-crate **registry** escape
while crates-io is replaced is what we would add.

## Where it lives (paths in rust-lang/cargo)

| Path | Role |
|------|------|
| `src/cargo/sources/config.rs` | Parses `[source.*]`, `replace-with`, builds `SourceConfigMap` |
| `src/cargo/sources/replaced.rs` | `ReplacedSource`: queries go to the replacement, ids stay original |
| `src/cargo/core/source_id.rs` | `SourceId` (crates-io vs alt registry vs git) |
| `src/cargo/util/toml/mod.rs` | Manifest `registry = "..."` on a dependency |
| `src/doc/src/reference/source-replacement.md` | The assumption we are changing |

docs.rs dump of `config.rs`:
https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/sources/config.rs.html

## What to change (product)

Add an **opt-in exception list** on the **replaced** source, not a silent
crates-io leak:

```toml
# .cargo/config.toml
[source.crates-io]
replace-with = "menhera-cooldown"
# These package names are resolved against the *original* crates-io
# sparse index. Everything else stays on the replacement.
allow-original = ["chacha20"]
```

Behavior:

1. Default: unchanged (all crates-io names go through `replace-with`).
2. If `name` is in `allow-original`, `SourceConfigMap::load` does **not**
   wrap that query in `ReplacedSource`; it uses the real crates-io
   `SourceId` (sparse `index.crates.io`).
3. **Lockfile:** those packages must **not** look like ordinary crates-io
   crates (or a later machine with no exception would fetch them from
   menhera). Encode the origin: e.g. source URL stays crates-io **and**
   a lockfile annotation, **or** treat them like an alt-registry id.
   This is the hard part. Do not ship an exception that is config-only
   while the lock still says `registry+https://github.com/rust-lang/crates.io-index`.
4. `cargo metadata` / `cargo tree` must show the exception.
5. Tests: `crates/cargo/tests/testsuite/` (source replacement tests
   already exist; add one crate allowed, one crate denied, yanked-on-
   mirror but present on origin).

## What not to change

- Do not remove `replace-with`.
- Do not make `--registry crates-io` the only escape (that is for
  `cargo publish` / API, RFC 3289, not resolve).
- Do not teach `[patch]` to fetch crates.io tarballs while replaced;
  git/path patch already works.

## Fork

```
git clone https://github.com/rust-lang/cargo
# fork on GitHub, then:
git remote add surmount git@github.com:SurmountSystems/cargo.git
```

Land the exception + tests + book paragraph. Consume via `rust-toolchain`
or a Surmount rustc/cargo overlay later; this note is the change list,
not a pin.
